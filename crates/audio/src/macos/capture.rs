//! Taking a Recording: an IO proc on the default input device for the microphone, and one on a
//! private aggregate device around a process tap for the Mac's own sound. The IO procs copy
//! each cycle into fixed-size blocks and hand them to the writer thread without blocking or
//! allocating; a full queue drops the block and counts it. With both tracks, the writer
//! removes the Mac's sound from the microphone before encoding it (ADR 0005).

use std::ffi::{CStr, c_void};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_audio::{
    AudioConvertHostTimeToNanos, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
    AudioObjectPropertyAddress, CATapDescription, kAudioAggregateDeviceIsPrivateKey,
    kAudioAggregateDeviceNameKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDefaultInputDevice,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioObjectUnknown, kAudioSubTapUIDKey,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp, AudioTimeStampFlags};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSNumber, NSString, NSUUID};

use super::echo::Echo;
use super::file::{AacFile, check};
use crate::{RecorderStatus, Track, downmix, is_silent};

/// Frames per block handed to the writer.
const BLOCK: usize = 1024;
/// Blocks the queue holds per track: about 10 s at 48 kHz, allocated once at start.
const QUEUE: usize = 512;
/// Microphone blocks the writer takes before it turns to the system queue, so a busy
/// microphone cannot starve it.
const BATCH: usize = 16;
/// How long the writer sleeps once both queues are empty. It polls, so an IO proc never has to
/// wake it, which could take a lock.
const WRITER_NAP: Duration = Duration::from_millis(20);

/// Up to [`BLOCK`] mono frames, copied by value through the queue.
struct Block {
    len: usize,
    /// The host time of the IO cycle's first frame, or 0 when Core Audio gave none.
    host_time: u64,
    /// Where the block starts in its cycle, in frames.
    offset: usize,
    samples: [f32; BLOCK],
}

impl Block {
    /// The host time of the block's first frame, in seconds.
    fn time(&self, sample_rate: f64) -> Option<f64> {
        if self.host_time == 0 {
            return None;
        }
        // SAFETY: a plain conversion with no preconditions.
        let nanos = unsafe { AudioConvertHostTimeToNanos(self.host_time) };
        Some(nanos as f64 / 1e9 + self.offset as f64 / sample_rate)
    }
}

/// What an IO proc reaches through its client pointer.
struct Feed {
    blocks: SyncSender<Block>,
    /// Frames dropped because the queue was full.
    dropped: AtomicU64,
    /// Set before the IO proc is stopped: it sends nothing more, even if Core Audio keeps
    /// calling it after a failed stop, so the writer can drain the queue and finish.
    closed: AtomicBool,
}

/// A running Recording. Dropping it stops and finalizes it, as [`Recorder::stop`] does.
pub struct Recorder {
    /// Stopped before the writer is waited for, so the queues close.
    captures: Vec<Capture>,
    writer: Option<JoinHandle<Vec<Written>>>,
    /// Set once the captures are stopped: the writer then closes each file when its queue is
    /// empty, even if an IO proc could not be unregistered and keeps its queue open.
    done: Arc<AtomicBool>,
    /// Whether each track, in [`Track::BOTH`] order, is being recorded.
    recording: [bool; 2],
    /// Whether each track, in [`Track::BOTH`] order, has heard more than silence.
    heard: [Arc<AtomicBool>; 2],
    started: Instant,
}

impl Recorder {
    /// Starts capturing into `dir` (created if missing): the microphone and the system tap, one
    /// file each. A track that cannot open or start is logged and left out, with no file; it
    /// errors only when neither starts. The first tap asks macOS for system audio access, and
    /// the first microphone read for microphone access, unless asked before.
    pub fn start(dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let heard = [Arc::default(), Arc::default()];
        let mut tracks = Vec::new();
        for (track, heard) in Track::BOTH.into_iter().zip(&heard) {
            match prepare(track, dir, heard) {
                Ok(prepared) => tracks.push(prepared),
                Err(error) => eprintln!("Recording: no {track:?} track: {error:#}"),
            }
        }
        // Started back to back, so the tracks begin within a few milliseconds of each other.
        tracks.retain_mut(|(_, capture)| {
            capture
                .start()
                .inspect_err(|error| {
                    eprintln!("Recording: no {:?} track: {error:#}", capture.track)
                })
                .is_ok()
        });
        let recording =
            Track::BOTH.map(|track| tracks.iter().any(|(written, _)| written.track == track));
        // A track left out keeps no file.
        for (track, recording) in Track::BOTH.into_iter().zip(recording) {
            if !recording {
                let _ = std::fs::remove_file(track.path(dir));
            }
        }
        if tracks.is_empty() {
            bail!("neither the microphone nor the Mac's sound can be recorded");
        }
        let (written, captures): (Vec<_>, Vec<_>) = tracks.into_iter().unzip();
        let done = Arc::new(AtomicBool::new(false));
        let writer = thread::Builder::new()
            .name("crumble-recording".into())
            .spawn({
                let done = Arc::clone(&done);
                move || write(written, &done)
            })?;
        Ok(Self {
            captures,
            writer: Some(writer),
            done,
            recording,
            heard,
            started: Instant::now(),
        })
    }

    /// Elapsed time and whether each recorded track has heard only digital silence so far.
    pub fn status(&self) -> RecorderStatus {
        let silent =
            |track: usize| self.recording[track] && !self.heard[track].load(Ordering::Relaxed);
        RecorderStatus {
            elapsed: self.started.elapsed(),
            mic_silent: silent(0),
            system_silent: silent(1),
        }
    }

    /// Stops, finalizes the files and returns the Recording's length: its longest track, zero
    /// when no audio arrived. Errors when a track could not be written or closed in full, its
    /// file keeping what was written before.
    pub fn stop(mut self) -> anyhow::Result<Duration> {
        self.finish()
    }

    fn finish(&mut self) -> anyhow::Result<Duration> {
        for capture in self.captures.drain(..) {
            let dropped = capture.feed.dropped.load(Ordering::Relaxed);
            if dropped > 0 {
                eprintln!(
                    "Recording: the {:?} track dropped {dropped} frames",
                    capture.track
                );
            }
        }
        self.done.store(true, Ordering::Release);
        let Some(writer) = self.writer.take() else {
            return Ok(Duration::ZERO);
        };
        let written = writer
            .join()
            .map_err(|_| anyhow!("the Recording's writer panicked"))?;
        if let Some((track, error)) = written
            .iter()
            .find_map(|track| Some((track.track, track.failed.as_ref()?)))
        {
            bail!("the {track:?} track stopped being written: {error:#}");
        }
        Ok(written
            .iter()
            .map(Written::length)
            .max()
            .unwrap_or_default())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("Recording: {error}");
        }
    }
}

/// A track's file as the writer fills it.
struct Written {
    track: Track,
    blocks: Receiver<Block>,
    /// `None` once stop was signalled, its queue emptied, and the file finalized.
    file: Option<AacFile>,
    frames: u64,
    sample_rate: f64,
    heard: Arc<AtomicBool>,
    /// The first write that failed. The file then takes no more, so it holds the audio up to
    /// the failure rather than skipping a gap.
    failed: Option<anyhow::Error>,
}

impl Written {
    /// Encodes `samples` into the file, unless an earlier write failed.
    fn write(&mut self, samples: &[f32]) {
        let Some(file) = &mut self.file else {
            return;
        };
        if self.failed.is_some() || samples.is_empty() {
            return;
        }
        match file.write(samples) {
            Ok(()) => self.frames += samples.len() as u64,
            Err(error) => {
                eprintln!("Recording: {:?}: {error:#}", self.track);
                self.failed = Some(error);
            }
        }
    }

    /// Finalizes the file; a failure to close is kept like a failed write.
    fn close(&mut self) {
        if let Some(Err(error)) = self.file.take().map(AacFile::close) {
            eprintln!("Recording: {:?}: {error:#}", self.track);
            self.failed.get_or_insert(error);
        }
    }

    fn length(&self) -> Duration {
        Duration::from_secs_f64(self.frames as f64 / self.sample_rate)
    }
}

/// The writer thread: drains each queue into its file until each is empty or closed once
/// `done` is set, then returns. The system queue goes first and is drained in full, as it is
/// the reference the microphone is cleaned against, so the microphone never gives up on a
/// reference still queued; then up to [`BATCH`] microphone blocks.
fn write(mut written: Vec<Written>, done: &AtomicBool) -> Vec<Written> {
    written.sort_by_key(|track| track.track == Track::Mic);
    let mut echo = canceller(&written);
    let mut cleaned = Vec::new();
    while written.iter().any(|track| track.file.is_some()) {
        // Read once per pass, so the system track is drained in full before the microphone
        // finishes against it.
        let stopping = done.load(Ordering::Acquire);
        let mut idle = true;
        for track in &mut written {
            if track.file.is_none() {
                continue;
            }
            let batch = match track.track {
                Track::System => QUEUE,
                Track::Mic => BATCH,
            };
            for _ in 0..batch {
                match track.blocks.try_recv() {
                    Ok(block) => {
                        idle = false;
                        let samples = &block.samples[..block.len];
                        if !is_silent(samples) {
                            track.heard.store(true, Ordering::Relaxed);
                        }
                        let at = block.time(track.sample_rate);
                        let samples = match (track.track, &mut echo) {
                            (Track::System, Some(echo)) => {
                                echo.reference(at, samples);
                                samples
                            }
                            // A file that failed takes nothing more, so nothing is cleaned for it.
                            (Track::Mic, Some(echo)) if track.failed.is_none() => {
                                cleaned.clear();
                                echo.mic(at, samples, &mut cleaned);
                                &cleaned
                            }
                            _ => samples,
                        };
                        track.write(samples);
                    }
                    // A queue is closed only once stop is signalled, so the system track has
                    // been drained in full before the microphone finishes against it.
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) if !stopping => break,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        if let (Track::Mic, Some(echo)) = (track.track, &mut echo) {
                            cleaned.clear();
                            echo.finish(&mut cleaned);
                            track.write(&cleaned);
                        }
                        track.close();
                        break;
                    }
                }
            }
        }
        if idle {
            thread::sleep(WRITER_NAP);
        }
    }
    written
}

/// The echo canceller for the microphone, when both tracks record.
fn canceller(written: &[Written]) -> Option<Echo> {
    let rate = |of: Track| {
        written
            .iter()
            .find(|track| track.track == of)
            .map(|track| track.sample_rate)
    };
    Echo::new(rate(Track::Mic)?, rate(Track::System)?, QUEUE * BLOCK)
        .inspect_err(|error| {
            eprintln!("Recording: the microphone keeps the Mac's sound: {error:#}")
        })
        .ok()
}

/// A track's file and its IO proc, registered but not started.
fn prepare(
    track: Track,
    dir: &Path,
    heard: &Arc<AtomicBool>,
) -> anyhow::Result<(Written, Capture)> {
    let source = Source::open(track)?;
    let sample_rate = source.sample_rate;
    let file = AacFile::create(&track.path(dir), sample_rate)?;
    let (sender, blocks) = sync_channel(QUEUE);
    let written = Written {
        track,
        blocks,
        file: Some(file),
        frames: 0,
        sample_rate,
        heard: Arc::clone(heard),
        failed: None,
    };
    Ok((written, Capture::new(track, source, sender)?))
}

/// Where a track's audio comes from.
struct Source {
    device: AudioObjectID,
    sample_rate: f64,
    /// For the system track: the tap and its aggregate device, destroyed with the capture.
    tap: Option<(AudioObjectID, AudioObjectID)>,
}

impl Source {
    fn open(track: Track) -> anyhow::Result<Self> {
        let (device, tap) = match track {
            Track::Mic => {
                let device: AudioObjectID = property(
                    kAudioObjectSystemObject as AudioObjectID,
                    kAudioHardwarePropertyDefaultInputDevice,
                )?;
                if device == kAudioObjectUnknown {
                    bail!("no input device");
                }
                (device, None)
            }
            Track::System => {
                let (tap, aggregate) = system_tap()?;
                (aggregate, Some((tap, aggregate)))
            }
        };
        // Built first, so an error below still destroys the tap.
        let mut source = Self {
            device,
            sample_rate: 0.0,
            tap,
        };
        source.sample_rate = property(device, kAudioDevicePropertyNominalSampleRate)?;
        if source.sample_rate <= 0.0 {
            bail!("the device reports no sample rate");
        }
        Ok(source)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if let Some((tap, aggregate)) = self.tap.take() {
            // SAFETY: both were created by `system_tap` and are destroyed only here.
            unsafe {
                AudioHardwareDestroyAggregateDevice(aggregate);
                AudioHardwareDestroyProcessTap(tap);
            }
        }
    }
}

/// A mono tap of every process's output except Crumble's, inside a private aggregate device
/// whose input is the tap. Returns the tap and the device.
fn system_tap() -> anyhow::Result<(AudioObjectID, AudioObjectID)> {
    // SAFETY: plain Objective-C calls on objects created here; the arrays hold the stated types.
    let description = unsafe {
        let description = CATapDescription::initMonoGlobalTapButExcludeProcesses(
            CATapDescription::alloc(),
            &NSArray::new(),
        );
        description.setPrivate(true);
        description.setName(&NSString::from_str("Crumble Recording"));
        // Leaves out Crumble's own sound, such as a Recording being played back.
        if let Some(bundle) = NSBundle::mainBundle().bundleIdentifier() {
            description.setBundleIDs(&NSArray::from_retained_slice(&[bundle]));
        }
        description
    };
    let mut tap = kAudioObjectUnknown;
    // SAFETY: a valid description, and `tap` receives the id.
    check(
        unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut tap) },
        "create the system audio tap",
    )?;
    // SAFETY: a getter on the description made above.
    let tap_uid = unsafe { description.UUID().UUIDString() };
    let sub_tap =
        NSDictionary::from_retained_objects(&[&*key(kAudioSubTapUIDKey)], &[any(tap_uid)]);
    let keys = [
        key(kAudioAggregateDeviceUIDKey),
        key(kAudioAggregateDeviceNameKey),
        key(kAudioAggregateDeviceIsPrivateKey),
        key(kAudioAggregateDeviceTapAutoStartKey),
        key(kAudioAggregateDeviceTapListKey),
    ];
    let values: [Retained<AnyObject>; 5] = [
        any(NSUUID::new().UUIDString()),
        any(NSString::from_str("Crumble Recording")),
        any(NSNumber::new_bool(true)),
        any(NSNumber::new_bool(true)),
        any(NSArray::from_retained_slice(&[sub_tap])),
    ];
    let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    let description = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: NSDictionary is toll-free bridged to CFDictionary.
    let description: &CFDictionary =
        unsafe { &*ptr::from_ref(&*description).cast::<CFDictionary>() };
    let mut aggregate = kAudioObjectUnknown;
    // SAFETY: a valid dictionary, and `aggregate` receives the id.
    let status =
        unsafe { AudioHardwareCreateAggregateDevice(description, NonNull::from(&mut aggregate)) };
    if let Err(error) = check(status, "create the system audio device") {
        // SAFETY: the tap was created above and is not used again.
        unsafe { AudioHardwareDestroyProcessTap(tap) };
        return Err(error);
    }
    Ok((tap, aggregate))
}

fn key(key: &CStr) -> Retained<NSString> {
    NSString::from_str(&key.to_string_lossy())
}

fn any<T: objc2::Message>(object: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(object) }
}

/// A global property of `object`, of type `T`.
fn property<T: Default>(object: AudioObjectID, selector: u32) -> anyhow::Result<T> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` holds `size` bytes of the property's type.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    check(status, "read a device property")?;
    Ok(value)
}

/// An IO proc registered on a source's device.
struct Capture {
    track: Track,
    source: Source,
    proc_id: AudioDeviceIOProcID,
    started: bool,
    /// Boxed so its address stays put while Core Audio holds it. Freed only once the IO proc is
    /// unregistered; leaked if that fails, as Core Audio may still call it.
    feed: ManuallyDrop<Box<Feed>>,
}

impl Capture {
    fn new(track: Track, source: Source, blocks: SyncSender<Block>) -> anyhow::Result<Self> {
        let feed = Box::new(Feed {
            blocks,
            dropped: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let mut proc_id: AudioDeviceIOProcID = None;
        // SAFETY: `feed` outlives the IO proc, which `Drop` destroys before freeing it.
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                source.device,
                Some(io_proc),
                ptr::from_ref(&*feed).cast_mut().cast(),
                NonNull::from(&mut proc_id),
            )
        };
        check(status, "register for audio")?;
        Ok(Self {
            track,
            source,
            proc_id,
            started: false,
            feed: ManuallyDrop::new(feed),
        })
    }

    fn start(&mut self) -> anyhow::Result<()> {
        // SAFETY: a registered IO proc on a live device.
        check(
            unsafe { AudioDeviceStart(self.source.device, self.proc_id) },
            "start recording",
        )?;
        self.started = true;
        Ok(())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.feed.closed.store(true, Ordering::Release);
        if self.started {
            // SAFETY: stops the IO proc started in `start`.
            let status = unsafe { AudioDeviceStop(self.source.device, self.proc_id) };
            if let Err(error) = check(status, "stop recording") {
                eprintln!("Recording: {:?}: {error:#}", self.track);
            }
        }
        // SAFETY: unregisters the IO proc created in `new`.
        let status = unsafe { AudioDeviceDestroyIOProcID(self.source.device, self.proc_id) };
        match check(status, "unregister from audio") {
            // SAFETY: Core Audio no longer calls the IO proc, and `feed` is dropped only here.
            Ok(()) => unsafe { ManuallyDrop::drop(&mut self.feed) },
            Err(error) => eprintln!("Recording: {:?}: {error:#}; leaking its queue", self.track),
        }
    }
}

/// Core Audio's real-time callback: downmixes the first input buffer to mono, in blocks, into
/// the queue. No locks, no allocation; the HAL hands IO procs `f32` samples.
unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the `Feed` registered with this proc, alive until it is destroyed,
    // and Core Audio passes a valid input list and time stamp for the cycle.
    let (feed, list, time) =
        unsafe { (&*client.cast::<Feed>(), input.as_ref(), input_time.as_ref()) };
    if feed.closed.load(Ordering::Acquire) {
        return 0;
    }
    // `mBuffers` holds `mNumberBuffers` entries, so the first exists only when that is nonzero.
    if list.mNumberBuffers == 0 {
        return 0;
    }
    let buffer = list.mBuffers[0];
    if buffer.mData.is_null() || buffer.mNumberChannels == 0 {
        return 0;
    }
    let channels = buffer.mNumberChannels as usize;
    // SAFETY: the buffer holds `mDataByteSize` bytes of `f32` samples.
    let samples = unsafe {
        std::slice::from_raw_parts(
            buffer.mData.cast::<f32>(),
            buffer.mDataByteSize as usize / size_of::<f32>(),
        )
    };
    let host_time = if time.mFlags.contains(AudioTimeStampFlags::HostTimeValid) {
        time.mHostTime
    } else {
        0
    };
    for (index, chunk) in samples.chunks(BLOCK * channels).enumerate() {
        let mut block = Block {
            len: 0,
            host_time,
            offset: index * BLOCK,
            samples: [0.0; BLOCK],
        };
        block.len = downmix(chunk, channels, &mut block.samples);
        if let Err(TrySendError::Full(block) | TrySendError::Disconnected(block)) =
            feed.blocks.try_send(block)
        {
            feed.dropped.fetch_add(block.len as u64, Ordering::Relaxed);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::sync_channel;

    use super::{AacFile, BLOCK, Block, QUEUE, Track, Written, write};

    /// A track whose queue holds `blocks` blocks without host times, then closes, as at stop.
    fn queued(track: Track, dir: &Path, blocks: usize) -> anyhow::Result<Written> {
        let (sender, receiver) = sync_channel(QUEUE);
        for _ in 0..blocks {
            sender.send(Block {
                len: BLOCK,
                host_time: 0,
                offset: 0,
                samples: [0.1; BLOCK],
            })?;
        }
        Ok(Written {
            track,
            blocks: receiver,
            file: Some(AacFile::create(&track.path(dir), 48_000.0)?),
            frames: 0,
            sample_rate: 48_000.0,
            heard: Default::default(),
            failed: None,
        })
    }

    #[test]
    fn stopping_writes_every_microphone_sample_even_past_its_reference() {
        let dir = std::env::temp_dir().join(format!("crumble-capture-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // The microphone runs past its reference but within the canceller's wait, so its tail
        // comes out only when the writer finishes it.
        let written = queued(Track::Mic, &dir, 20)
            .and_then(|mic| Ok(vec![mic, queued(Track::System, &dir, 4)?]))
            .map(|written| write(written, &AtomicBool::new(true)));
        let _ = std::fs::remove_dir_all(&dir);
        let written = written.unwrap_or_else(|error| panic!("{error:#}"));
        let frames = |of: Track| {
            written
                .iter()
                .find(|track| track.track == of)
                .map(|track| (track.frames, track.file.is_none(), track.failed.is_none()))
        };
        assert_eq!(frames(Track::Mic), Some((20 * BLOCK as u64, true, true)));
        assert_eq!(frames(Track::System), Some((4 * BLOCK as u64, true, true)));
    }
}
