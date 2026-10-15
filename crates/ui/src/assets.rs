use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

/// Embedded application artwork with explicit component icon replacements.
pub struct Assets;

const ICONS: &[(&str, &[u8])] = &[
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
        "icons/chat.svg",
        include_bytes!("../../../assets/hugeicons/BubbleChatIcon.svg"),
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
        "icons/help-circle.svg",
        include_bytes!("../../../assets/hugeicons/HelpCircleIcon.svg"),
    ),
    (
        "icons/home.svg",
        include_bytes!("../../../assets/hugeicons/Home01Icon.svg"),
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
        "icons/panel-left-close.svg",
        include_bytes!("../../../assets/hugeicons/LayoutLeftIcon.svg"),
    ),
    (
        "icons/panel-left-open.svg",
        include_bytes!("../../../assets/hugeicons/LayoutAlignLeftIcon.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../../../assets/hugeicons/Settings01Icon.svg"),
    ),
    (
        "icons/user-group.svg",
        include_bytes!("../../../assets/hugeicons/UserGroup02Icon.svg"),
    ),
    (
        "icons/video.svg",
        include_bytes!("../../../assets/hugeicons/Video01Icon.svg"),
    ),
    (
        "icons/view.svg",
        include_bytes!("../../../assets/hugeicons/ViewIcon.svg"),
    ),
    (
        "icons/view-off.svg",
        include_bytes!("../../../assets/hugeicons/ViewOffSlashIcon.svg"),
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
