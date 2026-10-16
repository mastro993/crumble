//! Records 3 s into a temporary folder, plays both tracks back together, prints what it got and
//! deletes the folder. Run by hand: it needs a microphone and asks for macOS permissions.
//!
//! ```sh
//! cargo run -p crumble-audio --example record
//! ```

use std::time::Duration;

use crumble_audio::{Player, Recorder, mic_access, recorded_length};

fn main() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("crumble-record-{}", std::process::id()));
    println!("Microphone access: {:?}", mic_access());
    let recorder = Recorder::start(&dir)?;
    std::thread::sleep(Duration::from_secs(3));
    let status = recorder.status();
    let length = recorder.stop()?;
    println!("Recorded {length:?}; {status:?}");
    for name in ["mic.aac", "system.aac"] {
        let size = std::fs::metadata(dir.join(name)).map(|file| file.len());
        println!("{name}: {size:?} bytes");
    }
    println!("Read back: {:?}", recorded_length(&dir)?);
    let mut player = Player::open(&dir)?;
    println!("Playing {:?}", player.duration());
    player.play();
    std::thread::sleep(Duration::from_millis(500));
    println!("Position after 0.5 s: {:?}", player.position());
    player.pause();
    drop(player);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
