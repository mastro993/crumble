use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

/// Embedded application artwork with explicit component icon replacements.
pub struct Assets;

const ICONS: &[(&str, &[u8])] = &[
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
        "icons/key.svg",
        include_bytes!("../../../assets/hugeicons/Key01Icon.svg"),
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

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if path.starts_with("icons/") {
            anyhow::bail!("Missing application icon mapping for {path}");
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        if path.starts_with("icons") {
            return Ok(ICONS
                .iter()
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
        ] {
            let path = icon.path();
            assert!(
                matches!(Assets.load(&path), Ok(Some(_))),
                "{path} has no mapping"
            );
        }
    }
}
