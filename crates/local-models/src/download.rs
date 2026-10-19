//! Downloads one file from Hugging Face into the models folder: resumed with HTTP Range from
//! the `.partial` file a cancelled or interrupted download left, checked against its sha256,
//! then renamed into place (ADR 0010).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{anyhow, bail};
use sha2::{Digest, Sha256};

use crate::{ModelFile, size_label};

/// How a download that did not fail ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file is in place and matches its sha256.
    Done,
    /// The user cancelled; the partial file stays for the next download.
    Cancelled,
}

/// Downloads `file` into `dir`, for `name`, the Model's name in errors. Blocks, so call it off
/// the main thread; `progress` gets the bytes held and the file's length; `cancel` stops it
/// between reads. Its error is a sentence for the user.
pub(crate) fn fetch(
    file: &ModelFile,
    dir: &Path,
    name: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> anyhow::Result<Outcome> {
    let target = dir.join(&file.filename);
    let partial = dir.join(format!("{}.partial", file.filename));
    fs::create_dir_all(dir).map_err(|error| saving(name, &error))?;
    let mut held = fs::metadata(&partial).map_or(0, |meta| meta.len());
    if held > file.size {
        fs::remove_file(&partial).map_err(|error| saving(name, &error))?;
        held = 0;
    }
    let available = fs4::available_space(dir).map_err(|error| saving(name, &error))?;
    if available < file.size - held {
        bail!("Not enough space: {name} needs {}.", size_label(file.size));
    }

    let mut hasher = Sha256::new();
    if held < file.size {
        let response = request(file, held)?;
        let status = response.status().as_u16();
        let resumed = match status {
            206 => true,
            200 => false,
            _ => bail!("Hugging Face turned the download down ({status})."),
        };
        let mut out = if resumed {
            hash_file(&partial, &mut hasher).map_err(|error| saving(name, &error))?;
            OpenOptions::new().append(true).open(&partial)
        } else {
            held = 0;
            File::create(&partial)
        }
        .map_err(|error| saving(name, &error))?;
        let mut body = response
            .into_body()
            .into_with_config()
            .limit(u64::MAX)
            .reader();
        let mut buffer = vec![0; 256 * 1024];
        progress(held, file.size);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(Outcome::Cancelled);
            }
            let read = body.read(&mut buffer).map_err(|error| {
                eprintln!("The download of {name} stopped: {error}");
                anyhow!("The download stopped. Try again to pick it up where it left off.")
            })?;
            if read == 0 {
                break;
            }
            let chunk = buffer.get(..read).unwrap_or_default();
            out.write_all(chunk).map_err(|error| saving(name, &error))?;
            hasher.update(chunk);
            held += read as u64;
            progress(held, file.size);
        }
        out.sync_all().map_err(|error| saving(name, &error))?;
    } else {
        hash_file(&partial, &mut hasher).map_err(|error| saving(name, &error))?;
    }

    if held < file.size {
        bail!("The download stopped. Try again to pick it up where it left off.");
    }
    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if held != file.size || sha256 != file.sha256 {
        let _ = fs::remove_file(&partial);
        bail!("The download was damaged. Try again.");
    }
    fs::rename(&partial, &target).map_err(|error| saving(name, &error))?;
    Ok(Outcome::Done)
}

/// GETs `file` from byte `from` on, whatever the status. Hugging Face redirects to its CDN,
/// which ureq follows with the Range header.
fn request(file: &ModelFile, from: u64) -> anyhow::Result<ureq::http::Response<ureq::Body>> {
    // ponytail: no idle timeout while the body streams, as ureq has none; a stalled connection
    // holds its thread until TCP gives up. Cancel takes effect at the next read.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .into();
    let mut request = agent.get(&file.url());
    if from > 0 {
        request = request.header("Range", format!("bytes={from}-"));
    }
    request.call().map_err(|error| {
        eprintln!(
            "Could not reach Hugging Face for {}: {error}",
            file.filename
        );
        anyhow!("Couldn't reach Hugging Face. Check the connection and try again.")
    })
}

/// Feeds the file at `path` to `hasher`, as a resumed download's sha256 covers its start too.
fn hash_file(path: &Path, hasher: &mut Sha256) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0; 256 * 1024];
    loop {
        match file.read(&mut buffer)? {
            0 => return Ok(()),
            read => hasher.update(buffer.get(..read).unwrap_or_default()),
        }
    }
}

/// What to tell the user when writing `name` to disk failed.
fn saving(name: &str, error: &io::Error) -> anyhow::Error {
    eprintln!("Could not save {name}: {error}");
    if error.kind() == io::ErrorKind::StorageFull {
        anyhow!("Not enough space to download {name}.")
    } else {
        anyhow!("Couldn't save {name} on this Mac.")
    }
}
