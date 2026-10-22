//! A select's menu: opening under its trigger on the picked row, and the search field that
//! hides the rows whose label lacks its text.

use super::super::controls::app_icon;
use super::{Kind, Menu, MenuItem, OpenMenu, step};
use crumble_theme::menu::{LINE_INK, SEPARATOR_MARGIN, TRAILING_SIZE};
use crumble_theme::sidebar::{ICON_GAP, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::{
    App, AppContext, Bounds, Context, Div, Focusable, InteractiveElement, ParentElement, Pixels,
    Point, Styled, Window, div, point, prelude::FluentBuilder, px,
};

/// Where a menu opens.
#[derive(Clone, Copy)]
pub(in crate::native_view) enum Place {
    /// Its top-left corner at this point in the window, as a context menu opens at the pointer.
    At(Point<Pixels>),
    /// Under the select drawn at these bounds, its trailing edge on the select's, at least as
    /// wide, and scrolling once it is too tall.
    Below(Bounds<Pixels>),
}

impl From<Point<Pixels>> for Place {
    fn from(point: Point<Pixels>) -> Self {
        Self::At(point)
    }
}

impl OpenMenu {
    /// Highlights row `ix` and scrolls it into view, as a select opens on its picked row.
    pub(in crate::native_view) fn highlight(self, ix: Option<usize>, cx: &mut App) -> Self {
        self.menu.update(cx, |menu, _| {
            menu.highlight =
                ix.filter(|&ix| menu.items.get(ix).is_some_and(|item| item.selectable()));
            if let Some(ix) = menu.highlight {
                menu.scroll.scroll_to_item(ix);
            }
        });
        self
    }

    /// Adds a search field above the rows and moves focus to it.
    pub(in crate::native_view) fn searchable(self, window: &mut Window, cx: &mut App) -> Self {
        self.menu.update(cx, |menu, cx| {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
            let filter = cx.subscribe(&search, |menu, search, event, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = search.read(cx).value().trim().to_lowercase();
                    menu.filter(&query);
                    cx.notify();
                }
            });
            window.focus(&search.focus_handle(cx), cx);
            menu._subscriptions.push(filter);
            menu.search = Some(search);
        });
        self
    }
}

impl Menu {
    /// Hides the rows whose label lacks `query`, and the separators while there is one, and
    /// highlights the first row left.
    fn filter(&mut self, query: &str) {
        hide_unmatched(&mut self.items, query);
        self.highlight = step(&self.items, None, true);
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    /// The search field and the line under it; the arrow keys, Enter and Escape typed into it
    /// move through and choose the rows, as they do on the menu.
    pub(super) fn render_search(&self, cx: &mut Context<Self>) -> Option<Div> {
        let search = self.search.as_ref()?;
        let theme = cx.theme();
        let (muted, line) = (theme.muted_foreground, theme.foreground.alpha(LINE_INK));
        Some(
            div()
                .flex_shrink_0()
                .child(
                    Input::new(search)
                        .appearance(false)
                        .prefix(app_icon("search").size(px(TRAILING_SIZE)).text_color(muted))
                        .h(px(ROW_HEIGHT))
                        .px(px(ROW_PADDING))
                        .gap(px(ICON_GAP)),
                )
                .child(div().h(px(1.)).my(px(SEPARATOR_MARGIN)).bg(line)),
        )
    }

    /// Takes the keys a search field would move its caret with, before it does.
    pub(super) fn search_keys<E: InteractiveElement + FluentBuilder>(
        &self,
        menu: E,
        cx: &mut Context<Self>,
    ) -> E {
        menu.when(self.search.is_some(), |menu| {
            menu.capture_action(
                cx.listener(|menu, _: &MoveUp, window, cx| menu.key("up", window, cx)),
            )
            .capture_action(
                cx.listener(|menu, _: &MoveDown, window, cx| menu.key("down", window, cx)),
            )
            .capture_action(
                cx.listener(|menu, _: &Enter, window, cx| menu.key("enter", window, cx)),
            )
            .capture_action(
                cx.listener(|menu, _: &Escape, window, cx| menu.key("escape", window, cx)),
            )
        })
    }
}

/// Hides the rows whose lowercased label lacks `query`, already lowercased, and every
/// separator while there is a query.
fn hide_unmatched(items: &mut [MenuItem], query: &str) {
    for item in items {
        item.hidden = !query.is_empty()
            && (matches!(item.kind, Kind::Separator) || !item.label.to_lowercase().contains(query));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MenuItem, step};
    use super::hide_unmatched;

    #[test]
    fn a_query_hides_the_rows_without_it_and_the_separators() {
        let mut items = [
            MenuItem::new("system", "System (English)"),
            MenuItem::separator(),
            MenuItem::new("afrikaans", "Afrikaans"),
            MenuItem::new("italian", "Italian"),
        ];
        hide_unmatched(&mut items, "ital");
        let hidden: Vec<bool> = items.iter().map(|item| item.hidden).collect();
        assert_eq!(hidden, [true, true, true, false]);
        assert_eq!(step(&items, None, true), Some(3));
        hide_unmatched(&mut items, "");
        assert!(items.iter().all(|item| !item.hidden));
        assert_eq!(step(&items, None, true), Some(0));
    }
}
