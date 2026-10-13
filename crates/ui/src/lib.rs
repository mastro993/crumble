//! Native application views and framework integration.

mod assets;
mod native_view;
mod settings;

pub use assets::Assets;
pub use native_view::{
    GoBack, GoForward, NativeView, NextPeriod, PreviousPeriod, ShowToday, ToggleSidebar,
};

/// Initializes the unmodified component library and application typography.
pub fn init(cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    crumble_theme::init(cx);
}
