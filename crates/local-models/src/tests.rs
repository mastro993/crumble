use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::download::fetch;
use crate::{Library, Model, ModelFile, Outcome, catalog, size_label, vad_file};

/// A fresh folder in the temp folder, deleted on drop.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("crumble-local-models-{}", Uuid::now_v7()));
        let _ = fs::create_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `model` as if downloaded into `library`: its file and its list entry.
fn place(library: &Library, model: &Model) {
    let _ = fs::write(library.path(model), b"weights");
    let entry = serde_json::to_vec(model).unwrap_or_default();
    let _ = fs::write(library.entry(&model.slug), entry);
}

fn file(contents: &[u8]) -> ModelFile {
    ModelFile {
        repo: "o/r".to_owned(),
        revision: "abc".to_owned(),
        filename: "m.gguf".to_owned(),
        size: contents.len() as u64,
        sha256: Sha256::digest(contents)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

#[test]
fn downloaded_models_list_in_catalog_order_then_dropped_ones() {
    let folder = Folder::new();
    let library = Library::at(folder.0.clone());
    assert!(library.downloaded().is_empty());
    let (first, second) = (catalog()[0].clone(), catalog()[1].clone());
    let mut dropped = first.clone();
    dropped.slug = "dropped-by-a-sync".to_owned();
    dropped.name = "A Dropped Model".to_owned();
    dropped.file.filename = "dropped.gguf".to_owned();
    place(&library, &second);
    place(&library, &dropped);
    place(&library, &first);
    // An entry whose file is gone, as when deleted in Finder, is not downloaded.
    let mut missing = catalog()[2].clone();
    missing.file.filename = "missing.gguf".to_owned();
    let _ = fs::write(
        library.entry(&missing.slug),
        serde_json::to_vec(&missing).unwrap_or_default(),
    );

    let downloaded = library.downloaded();
    let slugs: Vec<&str> = downloaded.iter().map(|d| d.model.slug.as_str()).collect();
    assert_eq!(
        slugs,
        [first.slug.as_str(), &second.slug, "dropped-by-a-sync"]
    );
    assert_eq!(
        downloaded.iter().map(|d| d.offered).collect::<Vec<_>>(),
        [true, true, false]
    );
    assert!(library.get(&missing.slug).is_none());
}

#[test]
fn deleting_the_last_model_deletes_the_vad_file() {
    let folder = Folder::new();
    let library = Library::at(folder.0.clone());
    let (first, second) = (catalog()[0].clone(), catalog()[1].clone());
    place(&library, &first);
    place(&library, &second);
    let _ = fs::write(library.vad(), b"vad");
    let _ = fs::write(
        folder.0.join(format!("{}.partial", first.file.filename)),
        b"part",
    );

    assert!(library.delete(&first.slug).is_ok());
    assert!(!library.path(&first).exists());
    assert!(
        !folder
            .0
            .join(format!("{}.partial", first.file.filename))
            .exists()
    );
    assert!(library.vad().exists());
    assert!(library.delete(&second.slug).is_ok());
    assert!(!library.vad().exists());
    assert!(library.delete("never-downloaded").is_ok());
}

#[test]
fn a_complete_partial_file_is_checked_and_moved_into_place() {
    let folder = Folder::new();
    let file = file(b"all of it");
    let partial = folder.0.join("m.gguf.partial");
    let _ = fs::write(&partial, b"all of it");
    let outcome = fetch(
        &file,
        &folder.0,
        "M",
        &mut |_, _| {},
        &AtomicBool::new(false),
    );
    assert_eq!(outcome.ok(), Some(Outcome::Done));
    assert!(folder.0.join("m.gguf").exists() && !partial.exists());
}

#[test]
fn a_damaged_download_is_deleted() {
    let folder = Folder::new();
    let file = file(b"all of it");
    let partial = folder.0.join("m.gguf.partial");
    let _ = fs::write(&partial, b"not of it");
    let outcome = fetch(
        &file,
        &folder.0,
        "M",
        &mut |_, _| {},
        &AtomicBool::new(false),
    );
    assert_eq!(
        outcome.map_err(|error| error.to_string()).err().as_deref(),
        Some("The download was damaged. Try again.")
    );
    assert!(!partial.exists() && !folder.0.join("m.gguf").exists());
}

#[test]
fn sizes_read_as_finder_shows_them() {
    assert_eq!(size_label(731_357_568), "732 MB");
    assert_eq!(size_label(1_770_000_000), "1.8 GB");
    assert_eq!(size_label(1), "1 MB");
}

/// Downloads the VAD file from Hugging Face, then resumes a cut-short copy of it.
#[test]
#[ignore = "downloads from Hugging Face"]
fn downloads_and_resumes_from_hugging_face() {
    let folder = Folder::new();
    let vad = vad_file();
    let cancel = AtomicBool::new(false);
    let fetched = fetch(&vad, &folder.0, "VAD", &mut |_, _| {}, &cancel);
    assert_eq!(
        fetched.map_err(|error| error.to_string()),
        Ok(Outcome::Done)
    );
    let whole = fs::read(folder.0.join(&vad.filename)).unwrap_or_default();
    let _ = fs::remove_file(folder.0.join(&vad.filename));
    let _ = fs::write(
        folder.0.join(format!("{}.partial", vad.filename)),
        whole.get(..500_000).unwrap_or_default(),
    );
    let mut first = None;
    let resumed = fetch(
        &vad,
        &folder.0,
        "VAD",
        &mut |held, _| {
            first.get_or_insert(held);
        },
        &cancel,
    );
    assert_eq!(
        resumed.map_err(|error| error.to_string()),
        Ok(Outcome::Done)
    );
    assert_eq!(first, Some(500_000));
}

/// Deleting the last other Model while one downloads takes the VAD file; the download fetches it
/// again before the Model shows as downloaded.
#[test]
#[ignore = "downloads from Hugging Face"]
fn a_download_fetches_the_vad_file_again_when_a_delete_took_it() {
    let folder = Folder::new();
    let library = Library::at(folder.0.clone());
    let other = catalog()[1].clone();
    place(&library, &other);
    let _ = fs::write(library.vad(), b"vad");
    let model = Model {
        file: file(b"weights"),
        ..catalog()[0].clone()
    };
    let _ = fs::write(folder.0.join("m.gguf.partial"), b"weights");
    let downloaded = library.download(
        &model,
        &mut |_, _| {
            let _ = library.delete(&other.slug);
        },
        &AtomicBool::new(false),
    );
    assert_eq!(
        downloaded.map_err(|error| error.to_string()),
        Ok(Outcome::Done)
    );
    assert_eq!(
        fs::metadata(library.vad()).map(|meta| meta.len()).ok(),
        Some(vad_file().size)
    );
    assert!(library.get(&model.slug).is_some());
}
