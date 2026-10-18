//! The player of the shown Note's finished Recording: its scrubber, Play and Pause, and seeking.

use super::super::super::NativeView;
use super::Playback;
use crumble_audio::Player;
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::{AppContext, Context, Task, Window};
use std::time::Duration;

/// How often the player's position redraws while it plays.
const PLAYBACK_TICK: Duration = Duration::from_millis(200);

impl NativeView {
    /// Opens the shown Note's finished Recording in the player, replacing any other.
    pub(in super::super) fn show_player(&mut self, cx: &mut Context<Self>) {
        let detail = &mut self.note_detail;
        detail.playback = None;
        let Some(note) = detail.note.as_ref() else {
            return;
        };
        if note
            .recording
            .is_none_or(|recording| recording.duration.is_none())
        {
            return;
        }
        let player = match Player::open(&self.notes.recording_dir(&note.id)) {
            Ok(player) => player,
            Err(error) => {
                eprintln!("Could not open the Recording: {error:#}");
                return;
            }
        };
        let length = player.duration().as_secs_f32().max(0.1);
        let scrubber = cx.new(|_| SliderState::new().min(0.0).max(length).step(0.1));
        let seek = cx.subscribe(&scrubber, |view, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(SliderValue::Single(to))
            | SliderEvent::Release(SliderValue::Single(to))) = *event
            else {
                return;
            };
            if let Some(playback) = &mut view.note_detail.playback {
                playback.player.seek(Duration::from_secs_f32(to.max(0.0)));
                cx.notify();
            }
        });
        self.note_detail.playback = Some(Playback {
            note: note.id.clone(),
            player,
            scrubber,
            _seek: seek,
            tick: Task::ready(()),
        });
    }

    pub(super) fn toggle_playback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(playback) = &mut self.note_detail.playback else {
            return;
        };
        if playback.player.is_playing() {
            playback.player.pause();
            playback.tick = Task::ready(());
            cx.notify();
            return;
        }
        playback.player.play();
        playback.tick = cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(PLAYBACK_TICK).await;
                let playing = view.update_in(cx, |view, window, cx| {
                    let Some(playback) = &mut view.note_detail.playback else {
                        return false;
                    };
                    let at = playback.player.position().as_secs_f32();
                    playback
                        .scrubber
                        .update(cx, |scrubber, cx| scrubber.set_value(at, window, cx));
                    cx.notify();
                    let playing = playback.player.is_playing();
                    if !playing {
                        // At the end: stops the engine, which runs until paused.
                        playback.player.pause();
                    }
                    playing
                });
                if !matches!(playing, Ok(true)) {
                    break;
                }
            }
        });
        cx.notify();
    }

    /// Moves the shown Note's player to `to`, as when a Transcript line's time is clicked.
    pub(in super::super) fn seek(
        &mut self,
        to: Duration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(playback) = &mut self.note_detail.playback else {
            return;
        };
        playback.player.seek(to);
        let at = playback.player.position().as_secs_f32();
        playback
            .scrubber
            .update(cx, |scrubber, cx| scrubber.set_value(at, window, cx));
        cx.notify();
    }
}
