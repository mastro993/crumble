//! The IO proc registered on a source's device, and the real-time callback that hands its
//! blocks to the writer without blocking or allocating (ADR 0005).

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};

use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioObjectID,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp, AudioTimeStampFlags};

use super::source::Source;
use super::{BLOCK, Block};
use crate::macos::file::check;
use crate::{Track, downmix};

/// What an IO proc reaches through its client pointer.
pub(super) struct Feed {
    blocks: SyncSender<Block>,
    /// Frames dropped because the queue was full.
    pub(super) dropped: AtomicU64,
    /// Set before the IO proc is stopped: it sends nothing more, even if Core Audio keeps
    /// calling it after a failed stop, so the writer can drain the queue and finish.
    closed: AtomicBool,
}

/// An IO proc registered on a source's device.
pub(super) struct Capture {
    pub(super) track: Track,
    source: Source,
    proc_id: AudioDeviceIOProcID,
    started: bool,
    /// Boxed so its address stays put while Core Audio holds it. Freed only once the IO proc is
    /// unregistered; leaked if that fails, as Core Audio may still call it.
    pub(super) feed: ManuallyDrop<Box<Feed>>,
}

impl Capture {
    pub(super) fn new(
        track: Track,
        source: Source,
        blocks: SyncSender<Block>,
    ) -> anyhow::Result<Self> {
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

    pub(super) fn start(&mut self) -> anyhow::Result<()> {
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
