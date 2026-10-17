//! Taking a Recording: an IO proc on the default input device for the microphone, and one on a
//! private aggregate device around a process tap for the Mac's own sound. The IO procs copy
//! each cycle into fixed-size blocks and hand them to the writer thread without blocking or
//! allocating; a full queue drops the block and counts it (ADR 0005).

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
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap,
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress, CATapDescription,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyNominalSampleRate,
    kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioObjectUnknown,
    kAudioSubTapUIDKey,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSNumber, NSString, NSUUID};

use super::file::{AacFile, check};
use crate::{RecorderStatus, Track, downmix, is_silent};

/// Frames per block handed to the writer.
const BLOCK: usize = 1024;
/// Blocks the queue holds per track: about 10 s at 48 kHz, allocated once at start.
const QUEUE: usize = 512;
/// Blocks the writer takes from one queue before it turns to the next, so a busy track cannot
/// starve the other.
const BATCH: usize = 16;
/// How long the writer sleeps once both queues are empty. It polls, so an IO proc never has to
/// wake it, which could take a lock.
const WRITER_NAP: Duration = Duration::from_millis(20);

/// Up to [`BLOCK`] mono frames, copied by value through the queue.
struct Block {
    len: usize,
    samples: [f32; BLOCK],
}

/// What an IO proc reaches through its client pointer.
struct Feed {
    blocks: SyncSender<Block>,
    /// Frames dropped because the queue was full.
    dropped: AtomicU64,
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

    /// Stops, finalizes the files and returns the Recording's length: its longest track. Errors
    /// when a track could not be written or closed in full, its file keeping what was written
    /// before, and when no audio arrived at all.
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
        let length = written
            .iter()
            .map(Written::length)
            .max()
            .unwrap_or_default();
        if length.is_zero() {
            bail!("no audio arrived before the Recording stopped");
        }
        Ok(length)
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
    /// `None` once its queue closed and the file was finalized.
    file: Option<AacFile>,
    frames: u64,
    sample_rate: f64,
    heard: Arc<AtomicBool>,
    /// The first write that failed. The file then takes no more, so it holds the audio up to
    /// the failure rather than skipping a gap.
    failed: Option<anyhow::Error>,
}

impl Written {
    #[allow(
        clippy::cast_precision_loss,
        reason = "frames of hours of audio fit in f64"
    )]
    fn length(&self) -> Duration {
        Duration::from_secs_f64(self.frames as f64 / self.sample_rate)
    }
}

/// The writer thread: drains every queue into its file, [`BATCH`] blocks at a time per queue,
/// until each closes, or is empty once `done` is set, then returns.
fn write(mut written: Vec<Written>, done: &AtomicBool) -> Vec<Written> {
    while written.iter().any(|track| track.file.is_some()) {
        let mut idle = true;
        for track in &mut written {
            let Some(file) = &mut track.file else {
                continue;
            };
            for _ in 0..BATCH {
                match track.blocks.try_recv() {
                    Ok(block) => {
                        idle = false;
                        if track.failed.is_some() {
                            continue;
                        }
                        let samples = &block.samples[..block.len];
                        if !is_silent(samples) {
                            track.heard.store(true, Ordering::Relaxed);
                        }
                        match file.write(samples) {
                            Ok(()) => track.frames += block.len as u64,
                            Err(error) => {
                                eprintln!("Recording: {:?}: {error:#}", track.track);
                                track.failed = Some(error);
                            }
                        }
                    }
                    Err(TryRecvError::Empty) if !done.load(Ordering::Acquire) => break,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        if let Some(Err(error)) = track.file.take().map(AacFile::close) {
                            eprintln!("Recording: {:?}: {error:#}", track.track);
                            track.failed.get_or_insert(error);
                        }
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
        // SAFETY: stops and unregisters the IO proc created in `new`.
        let status = unsafe {
            if self.started {
                AudioDeviceStop(self.source.device, self.proc_id);
            }
            AudioDeviceDestroyIOProcID(self.source.device, self.proc_id)
        };
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
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    // SAFETY: `client` is the `Feed` registered with this proc, alive until it is destroyed,
    // and Core Audio passes a valid input list for the cycle.
    let (feed, list) = unsafe { (&*client.cast::<Feed>(), input.as_ref()) };
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
    for chunk in samples.chunks(BLOCK * channels) {
        let mut block = Block {
            len: 0,
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
