//! A track's file: mono AAC in ADTS, written through ExtAudioFile. ADTS frames each carry their
//! own header, so whatever was written before a crash still plays; AAC in CAF or MP4 keeps its
//! packet table or `moov` atom for the end and is unreadable without it (ADR 0005).

use std::path::Path;
use std::ptr::{self, NonNull};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use objc2_audio_toolbox::{
    AudioFileFlags, ExtAudioFileCreateWithURL, ExtAudioFileDispose, ExtAudioFileGetProperty,
    ExtAudioFileOpenURL, ExtAudioFileRef, ExtAudioFileSetProperty, ExtAudioFileWrite,
    kAudioFileAAC_ADTSType, kExtAudioFileProperty_ClientDataFormat,
    kExtAudioFileProperty_FileDataFormat, kExtAudioFileProperty_FileLengthFrames,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, kAudioFormatFlagsNativeFloatPacked,
    kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
};
use objc2_core_foundation::CFURL;

/// An open track file being written.
pub(super) struct AacFile(ExtAudioFileRef);

// SAFETY: an ExtAudioFile may be used from any one thread at a time; the writer thread owns it.
unsafe impl Send for AacFile {}

impl AacFile {
    /// Creates `path`, replacing any file there, to hold mono audio at `sample_rate`, written as `f32`.
    pub(super) fn create(path: &Path, sample_rate: f64) -> anyhow::Result<Self> {
        let url = CFURL::from_file_path(path).ok_or_else(|| anyhow!("bad path {path:?}"))?;
        let mut aac = AudioStreamBasicDescription {
            mSampleRate: sample_rate,
            mFormatID: kAudioFormatMPEG4AAC,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: 1024,
            mBytesPerFrame: 0,
            mChannelsPerFrame: 1,
            mBitsPerChannel: 0,
            mReserved: 0,
        };
        let mut file: ExtAudioFileRef = ptr::null_mut();
        // SAFETY: the URL and the description outlive the call, and `file` receives the handle.
        let status = unsafe {
            ExtAudioFileCreateWithURL(
                &url,
                kAudioFileAAC_ADTSType,
                NonNull::from(&mut aac),
                ptr::null(),
                AudioFileFlags::EraseFile.0,
                NonNull::from(&mut file),
            )
        };
        check(status, "create the file")?;
        let file = Self(file);
        let mut client = float_format(sample_rate);
        // SAFETY: a live file, and a property of the size given.
        let status = unsafe {
            ExtAudioFileSetProperty(
                file.0,
                kExtAudioFileProperty_ClientDataFormat,
                size_of::<AudioStreamBasicDescription>() as u32,
                NonNull::from(&mut client).cast(),
            )
        };
        check(status, "set the client format")?;
        Ok(file)
    }

    /// Encodes and appends `samples`.
    pub(super) fn write(&mut self, samples: &[f32]) -> anyhow::Result<()> {
        let frames = u32::try_from(samples.len())?;
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: frames * 4,
                // ExtAudioFile reads the client buffer and never writes it.
                mData: samples.as_ptr().cast_mut().cast(),
            }],
        };
        // SAFETY: a live file, and a buffer list pointing at `frames` floats.
        let status = unsafe { ExtAudioFileWrite(self.0, frames, NonNull::from(&mut list)) };
        check(status, "write")
    }
}

impl AacFile {
    /// Flushes the encoder's last packets and closes the file, reporting whether that worked.
    pub(super) fn close(self) -> anyhow::Result<()> {
        let file = std::mem::ManuallyDrop::new(self);
        // SAFETY: the handle is live, and `ManuallyDrop` keeps `Drop` from disposing it again.
        check(unsafe { ExtAudioFileDispose(file.0) }, "close the file")
    }
}

impl Drop for AacFile {
    /// Closes a file never given to [`AacFile::close`], as when starting fails.
    fn drop(&mut self) {
        // SAFETY: the handle is live and disposed only here or in `close`.
        let status = unsafe { ExtAudioFileDispose(self.0) };
        if let Err(error) = check(status, "close the file") {
            eprintln!("Recording: {error}");
        }
    }
}

/// How much audio the file at `path` holds; it may have been cut short by a crash. ADTS keeps
/// no priming count, so this runs up to ~70 ms long.
pub(super) fn length(path: &Path) -> anyhow::Result<Duration> {
    let url = CFURL::from_file_path(path).ok_or_else(|| anyhow!("bad path {path:?}"))?;
    let mut file: ExtAudioFileRef = ptr::null_mut();
    // SAFETY: the URL outlives the call, and `file` receives the handle.
    check(
        unsafe { ExtAudioFileOpenURL(&url, NonNull::from(&mut file)) },
        "open the file",
    )?;
    let file = OpenFile(file);
    let mut frames: i64 = 0;
    let mut format = float_format(0.0);
    file.property(kExtAudioFileProperty_FileLengthFrames, &mut frames)?;
    file.property(kExtAudioFileProperty_FileDataFormat, &mut format)?;
    if format.mSampleRate <= 0.0 {
        bail!("{path:?} has no sample rate");
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "frames of hours of audio fit in f64"
    )]
    Ok(Duration::from_secs_f64(
        frames.max(0) as f64 / format.mSampleRate,
    ))
}

/// A file opened for reading, closed on drop.
struct OpenFile(ExtAudioFileRef);

impl OpenFile {
    fn property<T>(&self, id: u32, value: &mut T) -> anyhow::Result<()> {
        let mut size = size_of::<T>() as u32;
        // SAFETY: a live file, and `value` holds `size` bytes for a property of that type.
        let status = unsafe {
            ExtAudioFileGetProperty(
                self.0,
                id,
                NonNull::from(&mut size),
                NonNull::from(value).cast(),
            )
        };
        check(status, "read a property")
    }
}

impl Drop for OpenFile {
    fn drop(&mut self) {
        // SAFETY: the handle is live and disposed only here.
        unsafe { ExtAudioFileDispose(self.0) };
    }
}

/// Mono native-endian `f32`, the samples handed to the writer.
fn float_format(sample_rate: f64) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: sample_rate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagsNativeFloatPacked,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: 1,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

/// Turns a Core Audio status into an error naming what failed.
pub(super) fn check(status: i32, what: &str) -> anyhow::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(anyhow!("could not {what}: OSStatus {status}")).context("Core Audio")
    }
}

#[cfg(test)]
mod tests {
    use super::{AacFile, length};
    use std::f32::consts::TAU;
    use std::time::Duration;

    /// One second of a 440 Hz tone at 48 kHz.
    fn tone() -> Vec<f32> {
        (0..48_000)
            .map(|frame| (frame as f32 * TAU * 440.0 / 48_000.0).sin() * 0.25)
            .collect()
    }

    fn file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("crumble-audio-{}-{name}.aac", std::process::id()))
    }

    fn ok<T>(result: anyhow::Result<T>) -> T {
        result.unwrap_or_else(|error| panic!("{error:#}"))
    }

    #[test]
    fn a_closed_file_reads_back_its_length() {
        let path = file("closed");
        let mut aac = ok(AacFile::create(&path, 48_000.0));
        for _ in 0..3 {
            ok(aac.write(&tone()));
        }
        ok(aac.close());
        let read = length(&path);
        let _ = std::fs::remove_file(&path);
        let read = ok(read);
        // ADTS keeps no priming count, so the encoder's lead-in and last packet add up to ~70 ms.
        assert!(
            read.abs_diff(Duration::from_secs(3)) < Duration::from_millis(100),
            "{read:?}"
        );
    }

    #[test]
    fn a_file_never_closed_still_reads_what_was_written() {
        let path = file("crashed");
        let mut aac = ok(AacFile::create(&path, 48_000.0));
        for _ in 0..3 {
            ok(aac.write(&tone()));
        }
        // As if the app died here: the file is never finalized.
        std::mem::forget(aac);
        let read = length(&path);
        let _ = std::fs::remove_file(&path);
        let read = ok(read);
        assert!(read > Duration::from_millis(2_500), "{read:?}");
    }
}
