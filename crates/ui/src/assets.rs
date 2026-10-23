use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

/// Embedded application artwork with explicit component icon replacements, and the Providers'
/// logos.
pub struct Assets;

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/audio-wave.svg",
        include_bytes!("../../../assets/hugeicons/AudioWave01Icon.svg"),
    ),
    (
        "icons/chip.svg",
        include_bytes!("../../../assets/hugeicons/CpuIcon.svg"),
    ),
    (
        "icons/download.svg",
        include_bytes!("../../../assets/hugeicons/Download04Icon.svg"),
    ),
    (
        "icons/globe.svg",
        include_bytes!("../../../assets/hugeicons/Globe02Icon.svg"),
    ),
    (
        "icons/hard-drive.svg",
        include_bytes!("../../../assets/hugeicons/HardDriveIcon.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../../../assets/hugeicons/Search01Icon.svg"),
    ),
    (
        "icons/star.svg",
        include_bytes!("../../../assets/hugeicons/solid/StarIcon.svg"),
    ),
    (
        "icons/sort-ascending.svg",
        include_bytes!("../../../assets/hugeicons/Sorting02Icon.svg"),
    ),
    (
        "icons/sort-descending.svg",
        include_bytes!("../../../assets/hugeicons/Sorting01Icon.svg"),
    ),
    (
        "icons/voice.svg",
        include_bytes!("../../../assets/hugeicons/VoiceIcon.svg"),
    ),
    (
        "icons/agenda.svg",
        include_bytes!("../../../assets/hugeicons/NotebookTabsIcon.svg"),
    ),
    (
        "icons/agenda-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/NotebookTabsIcon.svg"),
    ),
    (
        "icons/ai-brain.svg",
        include_bytes!("../../../assets/hugeicons/AiBrain01Icon.svg"),
    ),
    (
        "icons/arrow-down.svg",
        include_bytes!("../../../assets/hugeicons/ArrowDown01Icon.svg"),
    ),
    (
        "icons/arrow-up.svg",
        include_bytes!("../../../assets/hugeicons/ArrowUp01Icon.svg"),
    ),
    (
        "icons/calendar.svg",
        include_bytes!("../../../assets/hugeicons/Calendar03Icon.svg"),
    ),
    (
        "icons/calendar-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/Calendar03Icon.svg"),
    ),
    (
        "icons/calendars.svg",
        include_bytes!("../../../assets/hugeicons/CalendarsIcon.svg"),
    ),
    (
        "icons/calendars-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/CalendarsIcon.svg"),
    ),
    (
        "icons/chat.svg",
        include_bytes!("../../../assets/hugeicons/BubbleChatIcon.svg"),
    ),
    (
        "icons/chat-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/BubbleChatIcon.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../../../assets/hugeicons/Tick02Icon.svg"),
    ),
    (
        "icons/chevron-down.svg",
        include_bytes!("../../../assets/hugeicons/ArrowDown01Icon.svg"),
    ),
    (
        "icons/chevron-left.svg",
        include_bytes!("../../../assets/hugeicons/ArrowLeft01Icon.svg"),
    ),
    (
        "icons/chevron-right.svg",
        include_bytes!("../../../assets/hugeicons/ArrowRight01Icon.svg"),
    ),
    (
        "icons/clock.svg",
        include_bytes!("../../../assets/hugeicons/Clock01Icon.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../../../assets/hugeicons/Cancel01Icon.svg"),
    ),
    (
        "icons/delete.svg",
        include_bytes!("../../../assets/hugeicons/Delete02Icon.svg"),
    ),
    (
        "icons/forward.svg",
        include_bytes!("../../../assets/hugeicons/ArrowTurnForwardIcon.svg"),
    ),
    (
        "icons/help-circle.svg",
        include_bytes!("../../../assets/hugeicons/HelpCircleIcon.svg"),
    ),
    (
        "icons/home.svg",
        include_bytes!("../../../assets/hugeicons/Home01Icon.svg"),
    ),
    (
        "icons/home-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/Home01Icon.svg"),
    ),
    (
        "icons/mic.svg",
        include_bytes!("../../../assets/hugeicons/Mic01Icon.svg"),
    ),
    (
        "icons/more.svg",
        include_bytes!("../../../assets/hugeicons/MoreHorizontalIcon.svg"),
    ),
    (
        "icons/note-add.svg",
        include_bytes!("../../../assets/hugeicons/NoteAddIcon.svg"),
    ),
    (
        "icons/notes.svg",
        include_bytes!("../../../assets/hugeicons/AppleReminderIcon.svg"),
    ),
    (
        "icons/paint-board.svg",
        include_bytes!("../../../assets/hugeicons/PaintBoardIcon.svg"),
    ),
    (
        "icons/pencil.svg",
        include_bytes!("../../../assets/hugeicons/PencilEdit01Icon.svg"),
    ),
    (
        "icons/pause.svg",
        include_bytes!("../../../assets/hugeicons/PauseIcon.svg"),
    ),
    (
        "icons/panel-left-close.svg",
        include_bytes!("../../../assets/hugeicons/LayoutLeftIcon.svg"),
    ),
    (
        "icons/panel-left-open.svg",
        include_bytes!("../../../assets/hugeicons/LayoutAlignLeftIcon.svg"),
    ),
    (
        "icons/play.svg",
        include_bytes!("../../../assets/hugeicons/PlayIcon.svg"),
    ),
    (
        "icons/refresh.svg",
        include_bytes!("../../../assets/hugeicons/RefreshIcon.svg"),
    ),
    (
        "icons/scroll-to-top.svg",
        include_bytes!("../../../assets/hugeicons/ArrowUp02Icon.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../../../assets/hugeicons/Settings01Icon.svg"),
    ),
    (
        "icons/settings-selected.svg",
        include_bytes!("../../../assets/hugeicons/solid/Settings01Icon.svg"),
    ),
    (
        "icons/stop.svg",
        include_bytes!("../../../assets/hugeicons/StopIcon.svg"),
    ),
    (
        "icons/user-group.svg",
        include_bytes!("../../../assets/hugeicons/UserGroup02Icon.svg"),
    ),
    (
        "icons/video.svg",
        include_bytes!("../../../assets/hugeicons/Video01Icon.svg"),
    ),
];

/// The Providers' logos, each named after its Provider's id (see
/// `assets/lobehub/README.md`).
const LOGOS: &[(&str, &[u8])] = &[
    (
        "logos/apple.svg",
        include_bytes!("../../../assets/lobehub/apple.svg"),
    ),
    (
        "logos/anthropic.svg",
        include_bytes!("../../../assets/lobehub/anthropic.svg"),
    ),
    (
        "logos/deepseek.svg",
        include_bytes!("../../../assets/lobehub/deepseek.svg"),
    ),
    (
        "logos/elevenlabs.svg",
        include_bytes!("../../../assets/lobehub/elevenlabs.svg"),
    ),
    (
        "logos/gemini.svg",
        include_bytes!("../../../assets/lobehub/gemini.svg"),
    ),
    (
        "logos/openai.svg",
        include_bytes!("../../../assets/lobehub/openai.svg"),
    ),
    (
        "logos/openrouter.svg",
        include_bytes!("../../../assets/lobehub/openrouter.svg"),
    ),
    (
        "logos/xai.svg",
        include_bytes!("../../../assets/lobehub/xai.svg"),
    ),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().chain(LOGOS).find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if path.starts_with("icons/") {
            anyhow::bail!("Missing application icon mapping for {path}");
        }
        if path.starts_with("logos/") {
            anyhow::bail!("Missing Provider logo mapping for {path}");
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        if path.starts_with("icons") || path.starts_with("logos") {
            return Ok(ICONS
                .iter()
                .chain(LOGOS)
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name))
                .collect());
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::Assets;
    use gpui_kit::AssetSource;
    use gpui_kit::component::{IconName, IconNamed};

    /// Components draw these themselves; an unmapped one renders as nothing, such as a
    /// checkbox with no tick.
    #[test]
    fn component_icons_the_app_uses_are_mapped() {
        for icon in [
            IconName::Check,
            IconName::ChevronDown,
            IconName::ChevronRight,
            IconName::Close,
        ] {
            let path = icon.path();
            assert!(
                matches!(Assets.load(&path), Ok(Some(_))),
                "{path} has no mapping"
            );
        }
    }

    /// An unmapped icon or logo is a mistake to fix, not artwork to skip.
    #[test]
    fn an_unmapped_icon_or_logo_fails_to_load() {
        assert!(Assets.load("icons/missing.svg").is_err());
        assert!(Assets.load("logos/missing.svg").is_err());
        assert!(matches!(Assets.load("elsewhere/missing.svg"), Ok(None)));
    }
}
