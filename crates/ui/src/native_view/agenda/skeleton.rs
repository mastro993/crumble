//! The Agenda list before its first read finishes: placeholder rows shaped like a day header and
//! its Events, so the list does not pop in from nothing.

use super::row::CARD_INSET;
use crumble_theme::CONTENT_PADDING;
use crumble_theme::sidebar::ROW_PADDING;
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{IntoElement, ParentElement, Styled, div, px};

/// Placeholder Events, enough to fill a tall window.
const ROWS: usize = 8;
/// Widths of the placeholder titles, in turn, so the rows do not look stamped.
const TITLE_WIDTHS: [f32; 4] = [0.7, 0.5, 0.8, 0.6];

/// Placeholder rows: a day header, then Events with a bar, a title, and a time, spaced as the
/// real rows are.
pub(super) fn agenda_skeleton() -> impl IntoElement {
    v_flex()
        .size_full()
        .child(
            div().px(px(CONTENT_PADDING)).pt_4().pb_1().child(
                Skeleton::new()
                    .w(px(56.0))
                    .h(px(14.0))
                    .my(px(3.0))
                    .rounded_sm(),
            ),
        )
        .children((0..ROWS).map(|index| {
            let title = TITLE_WIDTHS[index % TITLE_WIDTHS.len()];
            div().px(px(CARD_INSET)).child(
                h_flex()
                    .px(px(ROW_PADDING))
                    .py_1p5()
                    .gap_3()
                    .items_stretch()
                    .child(
                        Skeleton::new()
                            .secondary()
                            .flex_shrink_0()
                            .w(px(3.0))
                            .h_auto()
                            .rounded_full(),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.0))
                            .py(px(6.0))
                            .child(
                                Skeleton::new()
                                    .w(gpui_kit::relative(title))
                                    .h(px(14.0))
                                    .rounded_sm(),
                            )
                            .child(
                                Skeleton::new()
                                    .secondary()
                                    .w(px(80.0))
                                    .h(px(10.0))
                                    .rounded_sm(),
                            ),
                    ),
            )
        }))
}
