//! Local Models: the Models Crumble downloads and runs itself, on the Mac (ADR 0010). This
//! crate manages them: the list from Handy, the models folder, downloads and deletion. Running
//! one is `crumble-transcribe`'s job. No GPUI.

mod catalog;
mod download;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::Context;

pub use catalog::{Filter, Model, ModelFile, catalog, find};
pub use download::Outcome;

/// Silero VAD, which finds the pauses where long audio is cut for Models that cannot take it
/// whole. Downloaded with the first Local Model, hidden from the list.
fn vad_file() -> ModelFile {
    ModelFile {
        repo: "handy-computer/silero-vad-v6.2-gguf".to_owned(),
        revision: "a9b2d61c20ae4ddf3644edb34969e8185a485448".to_owned(),
        filename: "silero-vad-v6.2-F32.gguf".to_owned(),
        size: 1_241_056,
        sha256: "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6".to_owned(),
    }
}

/// Held while a download fetches the VAD file or saves its list entry, and while a delete decides
/// whether the VAD file goes: so downloads running at once never fetch it into one partial file
/// together, and a delete never takes it from a Model still downloading.
static VAD: Mutex<()> = Mutex::new(());

fn lock_vad() -> MutexGuard<'static, ()> {
    VAD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A downloaded Local Model.
#[derive(Clone, Debug, PartialEq)]
pub struct Downloaded {
    /// The Model as its list entry read when it was downloaded.
    pub model: Model,
    /// Whether the list still offers it; one a later sync dropped shows as "No longer offered".
    pub offered: bool,
}

/// The models folder: each downloaded Model's file, with a copy of its list entry beside it as
/// `<slug>.json`, and the VAD file.
#[derive(Clone, Debug)]
pub struct Library {
    dir: PathBuf,
}

impl Library {
    /// `models/` in Crumble's data folder, `~/Library/Application Support/Crumble` on macOS, the
    /// folder `crumble-db` keeps the database in.
    pub fn open() -> anyhow::Result<Self> {
        let data = dirs::data_dir().context("this platform has no data folder")?;
        Ok(Self::at(data.join("Crumble").join("models")))
    }

    /// The models folder at `dir`, created when the first download starts.
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Every downloaded Model: those the list offers in its order, then the others by name.
    pub fn downloaded(&self) -> Vec<Downloaded> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut downloaded: Vec<Downloaded> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == "json").then_some(())?;
                let model: Model = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                self.path(&model).exists().then(|| Downloaded {
                    offered: find(&model.slug).is_some(),
                    model,
                })
            })
            .collect();
        let place = |slug: &str| catalog().iter().position(|model| model.slug == slug);
        downloaded.sort_by(|a, b| {
            let (a_place, b_place) = (place(&a.model.slug), place(&b.model.slug));
            (a_place.is_none(), a_place, &a.model.name).cmp(&(
                b_place.is_none(),
                b_place,
                &b.model.name,
            ))
        });
        downloaded
    }

    /// The downloaded Model `slug`.
    pub fn get(&self, slug: &str) -> Option<Downloaded> {
        self.downloaded()
            .into_iter()
            .find(|downloaded| downloaded.model.slug == slug)
    }

    /// Where `model`'s file is, or goes.
    pub fn path(&self, model: &Model) -> PathBuf {
        self.dir.join(&model.file.filename)
    }

    /// Where the VAD file is.
    pub fn vad(&self) -> PathBuf {
        self.dir.join(vad_file().filename)
    }

    /// Downloads `model`, and the VAD file first when missing, then keeps a copy of its list
    /// entry. Blocks, so call it off the main thread; `progress` gets the bytes held and the
    /// file's length; `cancel` stops it between reads, keeping what arrived for next time. Its
    /// error is a sentence for the user.
    pub fn download(
        &self,
        model: &Model,
        progress: &mut dyn FnMut(u64, u64),
        cancel: &AtomicBool,
    ) -> anyhow::Result<Outcome> {
        if self.fetch_vad(&model.name, cancel, &lock_vad())? == Outcome::Cancelled {
            return Ok(Outcome::Cancelled);
        }
        let outcome = download::fetch(&model.file, &self.dir, &model.name, progress, cancel)?;
        if outcome == Outcome::Cancelled {
            return Ok(outcome);
        }
        // Deleting the last other Model while this one downloaded took the VAD file with it.
        // The Model is in place by now, so this fetch is not cancelled.
        let vad = lock_vad();
        self.fetch_vad(&model.name, &AtomicBool::new(false), &vad)?;
        let entry =
            serde_json::to_vec_pretty(model).context("Couldn't save the Model's details.")?;
        fs::write(self.entry(&model.slug), entry).map_err(|error| {
            eprintln!("Could not save {}'s list entry: {error}", model.name);
            anyhow::anyhow!("Couldn't save {} on this Mac.", model.name)
        })?;
        Ok(Outcome::Done)
    }

    /// Fetches the VAD file, for `name`, unless it is there; `_vad` shows the lock is held.
    fn fetch_vad(
        &self,
        name: &str,
        cancel: &AtomicBool,
        _vad: &MutexGuard<'static, ()>,
    ) -> anyhow::Result<Outcome> {
        if self.vad().exists() {
            return Ok(Outcome::Done);
        }
        download::fetch(&vad_file(), &self.dir, name, &mut |_, _| {}, cancel)
    }

    /// Deletes the downloaded Model `slug`, with what a cancelled download of it left, and the
    /// VAD file once no Model is left.
    pub fn delete(&self, slug: &str) -> anyhow::Result<()> {
        if let Some(Downloaded { model, .. }) = self.get(slug) {
            remove(&self.path(&model))?;
            remove(&self.dir.join(format!("{}.partial", model.file.filename)))?;
        }
        remove(&self.entry(slug))?;
        let _vad = lock_vad();
        if self.downloaded().is_empty() {
            remove(&self.vad())?;
        }
        Ok(())
    }

    fn entry(&self, slug: &str) -> PathBuf {
        self.dir.join(format!("{slug}.json"))
    }
}

/// Removes the file at `path` if there is one.
fn remove(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("Couldn't delete {}", path.display()))
        }
        _ => Ok(()),
    }
}

/// `bytes` for the user, in decimal units as Finder shows them: "832 MB", "1.8 GB".
pub fn size_label(bytes: u64) -> String {
    const MB: u64 = 1_000_000;
    if bytes >= 1000 * MB {
        format!("{:.1} GB", bytes as f64 / (1000 * MB) as f64)
    } else {
        format!("{} MB", bytes.div_ceil(MB))
    }
}

#[cfg(test)]
mod tests;
