use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

/// Embedded application artwork with explicit component icon replacements.
pub struct Assets;

const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/apple.svg",
        include_bytes!("../../../assets/hugeicons/AppleIcon.svg"),
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
        "icons/google.svg",
        include_bytes!("../../../assets/hugeicons/GoogleIcon.svg"),
    ),
    (
        "icons/home.svg",
        include_bytes!("../../../assets/hugeicons/Home01Icon.svg"),
    ),
    (
        "icons/microsoft.svg",
        include_bytes!("../../../assets/hugeicons/MicrosoftIcon.svg"),
    ),
    (
        "icons/more.svg",
        include_bytes!("../../../assets/hugeicons/MoreHorizontalIcon.svg"),
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
        for icon in [IconName::Check, IconName::ChevronDown] {
            let path = icon.path();
            assert!(
                matches!(Assets.load(&path), Ok(Some(_))),
                "{path} has no mapping"
            );
        }
    }
}
