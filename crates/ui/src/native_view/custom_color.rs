//! The Custom level of an Account's Color menu: GPUI Kit's headless color picker, drawn as a menu
//! panel with a palette of shades and a hex field.

use gpui_kit::base::ColorSwatch;
use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme, ColorName, Colorize, Sizable, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, Hsla, InteractiveElement,
    IntoElement, ParentElement, SharedString, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

/// One row of shades per hue, as GPUI Kit's own color picker lays them out.
const HUES: [ColorName; 9] = [
    ColorName::Neutral,
    ColorName::Red,
    ColorName::Orange,
    ColorName::Yellow,
    ColorName::Green,
    ColorName::Cyan,
    ColorName::Blue,
    ColorName::Purple,
    ColorName::Pink,
];
/// Darkest to lightest.
const SHADES: [usize; 11] = [950, 900, 800, 700, 600, 500, 400, 300, 200, 100, 50];
const SWATCH_SIZE: f32 = 20.0;
const SWATCH_GAP: f32 = 4.0;

/// GPUI Kit's headless color picker, holding an Account's color while its menu is open.
pub(super) struct CustomColor {
    picker: Entity<ColorPickerState>,
    /// The hex field, which the keyboard reaches first.
    focus: FocusHandle,
    _changes: Subscription,
}

impl CustomColor {
    /// Holds `current` for a view `V`. `pick` runs with the color the user picked, `None` when
    /// it is still `current`, and `done` set once the pick should end the menu, as choosing a
    /// swatch or entering a hex value does.
    pub(super) fn new<V: 'static>(
        current: Hsla,
        window: &mut Window,
        cx: &mut Context<V>,
        pick: impl Fn(&mut V, Option<Hsla>, bool, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let picker = cx.new(|cx| {
            let mut picker = ColorPickerState::new(window, cx).default_value(current);
            // Choosing a swatch or entering a hex value closes the picker, which ends the pick.
            picker.set_open(true, cx);
            picker
        });
        let focus = picker.read(cx).hex_input().focus_handle(cx);
        let changes = cx.observe_in(&picker, window, move |view, picker, window, cx| {
            let picker = picker.read(cx);
            let (picked, done) = (
                picker.value().filter(|&color| color != current),
                !picker.is_open(),
            );
            if picked.is_some() || done {
                pick(view, picked, done, window, cx);
            }
        });
        Self {
            picker,
            focus,
            _changes: changes,
        }
    }

    pub(super) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    /// The shades, then the hex field beside the color it holds or the hovered shade.
    pub(super) fn panel(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        // A starting color reaches the hex field only once a window can write it.
        self.picker
            .update(cx, |picker, cx| picker.sync_pending_value(window, cx));
        let picker = self.picker.read(cx);
        let (value, shown) = (picker.value(), picker.displayed_color());
        let (hex, radius) = (picker.hex_input().clone(), cx.theme().radius);
        let rows = HUES.map(|hue| {
            h_flex()
                .gap(px(SWATCH_GAP))
                .children(SHADES.map(|shade| self.swatch(hue.scale(shade), value, cx)))
        });
        v_flex()
            .gap(px(SWATCH_GAP))
            .children(rows)
            .child(
                h_flex()
                    .mt_2()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(SWATCH_SIZE))
                            .rounded(radius)
                            .when_some(shown, |preview, color| {
                                preview.bg(color).border_1().border_color(color.darken(0.2))
                            }),
                    )
                    .child(Input::new(&hex).small().flex_1()),
            )
            .into_any_element()
    }

    /// A shade that previews while hovered and is picked when clicked.
    fn swatch(&self, color: Hsla, value: Option<Hsla>, cx: &App) -> ColorSwatch {
        let (preview, select) = (self.picker.clone(), self.picker.clone());
        let ring = cx.theme().foreground;
        ColorSwatch::new(
            SharedString::from(format!("custom-color-{}", color.to_hex())),
            color,
        )
        .selected(value == Some(color))
        .size(px(SWATCH_SIZE))
        .bg(color)
        .border_1()
        .border_color(color.darken(0.1))
        .when(value == Some(color), |swatch| {
            swatch.border_2().border_color(ring)
        })
        .hover(move |style| style.border_color(color.darken(0.3)))
        .cursor_pointer()
        .on_hover(move |color, entered, window, cx| {
            if entered {
                preview.update(cx, |picker, cx| picker.preview_color(color, window, cx));
            }
        })
        .on_click(move |color, _, window, cx| {
            select.update(cx, |picker, cx| picker.select_color(color, window, cx));
        })
    }
}
