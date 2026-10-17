# Recording with a process tap

A Note's Recording holds what was said in a meeting: the user's microphone and the call coming out of the Mac. Crumble records it as two tracks: the default input device, and a Core Audio process tap of every process's output except Crumble's own (`CATapDescription` initialised with `initMonoGlobalTapButExcludeProcesses:`, Crumble excluded by its bundle ID, made private, wrapped in a private aggregate device). Each track gets an IO proc registered with `AudioDeviceCreateIOProcID`. The bindings are `objc2-core-audio`, `objc2-audio-toolbox` and `objc2-avf-audio`, the family `crumble-eventkit` already uses; they cover taps, aggregate devices, ExtAudioFile and AVAudioEngine, so no other binding crate is needed. anarlog, an open-source meeting-notes app for macOS, builds its tap the same way through `cidre`.

## Rules

- **Two tracks, kept apart**: `mic.aac` and `system.aac` in `recordings/<note id>/` beside `crumble.sqlite3`. Transcription can then tell the user from the other side, and a silent track points at the permission that is missing. The tracks start back to back and may begin a few milliseconds apart; nothing aligns them more finely.
- **Mono AAC-LC in ADTS, at the device's sample rate** (usually 48 kHz, never resampled). ADTS frames each carry their own header, so a file cut off by a crash or force-quit plays up to its last packet. AAC in CAF was the first choice, but a CAF file killed mid-write read back as 0 packets, as its packet table is written at close; an `.m4a` needs its trailing `moov` atom. A test writes a file, never closes it, and reads it back. ExtAudioFile counts ADTS frames exactly; ADTS keeps no priming count, so a length read from a file runs up to ~70 ms long.
- **Callbacks never block.** An IO proc averages the first input buffer's channels to mono, cuts it into 1024-frame blocks, and `try_send`s each by value over a bounded `std::sync::mpsc::sync_channel` of 512 blocks (about 10 s at 48 kHz), allocated once at start. A full channel drops the block and counts its frames; the count is logged at stop. The writer thread takes up to 16 blocks from one channel before turning to the other, so a busy track cannot starve the other, and sleeps 20 ms once both are empty instead of blocking on them, so an IO proc never has to wake it, which could take a lock. It encodes with `ExtAudioFileWrite`, marks a track heard once it gets anything but digital silence, and finalizes each file when its channel closes. A failed write ends that track's file there, rather than leaving a gap, and `Recorder::stop` returns the error.
- **Permissions.** `Info.plist` carries `NSMicrophoneUsageDescription` and `NSAudioCaptureUsageDescription`. Record asks for the microphone through `AVAudioApplication` first, because registering an IO proc on the input device blocks while macOS's prompt is open. There is no public way to ask for, or read, system audio access: macOS prompts on the first tap and, until it is granted, the tap delivers silence. So silence is the signal: a track whose every sample stays under 1e-6 for the first 10 s brings up a line pointing to the System Settings pane that allows it. No private TCC API is used.
- **Playback**: one `AVAudioEngine`, a player node per track, both scheduled from the same position and started at the same host time; a seek stops and reschedules both. `AVAudioPlayer` was simpler, but it estimates an ADTS file's length from its bit rate: a 600 s file came back as 793 s.
- **Storage**: the database keeps `recording_started_at` and `recording_duration_ms`; a start without a length is a Recording still running or cut short by a crash. At launch, `Notes::recover_recordings` stores what each such Recording's files hold, and deletes one with nothing readable. `crumble-notes` owns the folder and its deletion; `crumble-audio` takes a folder and owns the file names in it.

## Considered Options

- **ScreenCaptureKit**: captures system audio too, but asks for Screen Recording, a broader permission than audio alone, and is built around display capture.
- **A virtual device such as BlackHole**: an install and a change to the user's output device, which is too much to ask for a calendar app.
- **One mixed track**: smaller, but transcription could not tell the speakers apart, and silence could not be traced to one permission.
- **WAV, as anarlog writes and later encodes**: crash-tolerant once its header is patched, but about 690 MB per track per hour of 32-bit float at 48 kHz.
- **A lock-free ring buffer crate** (`ringbuf`, `rtrb`): what anarlog uses. A bounded std channel of fixed-size blocks, polled by the writer, keeps the IO proc lock- and allocation-free without a dependency.
- **AVAudioEngine's input node for the microphone**: one more object graph, while the IO proc already written for the tap serves the input device unchanged.

## Consequences

- About 65 kbps per speech track at the encoder's default bit rate, so about 30 MB per track and 60 MB per hour of Recording; silence costs far less.
- Headphones are assumed: there is no echo cancellation, so on speakers the microphone track also hears the call.
- macOS 14.2 or later for process taps, and macOS 26 for excluding Crumble by bundle ID; the app already requires macOS 26.
- The input device is the default one when Record is pressed. Switching devices mid-Recording, as by connecting headphones, leaves that track silent or short; following the default device is a follow-up.
- Starting and stopping the devices runs on the main thread and can take a moment.
- `crumble-audio` uses `unsafe` for every Core Audio, AudioToolbox and AVFAudio call, so like `crumble-eventkit` (ADR 0001) it sets `unsafe_code = "deny"` with a `SAFETY` comment on each block, instead of the workspace's `forbid`.
- The Recording stays on the Mac until the user deletes it, or deletes its Note.
