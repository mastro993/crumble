//! How a menu draws: the backdrop, the menu, submenu, and panel surfaces, and their rows.

use super::super::controls::app_icon;
use super::{Kind, Leading, Menu, MenuItem};
use crumble_theme::SURFACE_RADIUS;
use crumble_theme::menu::{
    DISABLED_OPACITY, HIGHLIGHT_INK, LABEL_SIZE, LINE_INK, MIN_WIDTH, PADDING, SEPARATOR_MARGIN,
    SUBMENU_OVERLAP, TRAILING_SIZE, WINDOW_MARGIN, shadow, surface,
};
use crumble_theme::sidebar::{ICON_GAP, ICON_SIZE, ROW_HEIGHT, ROW_PADDING};
use gpui_kit::base::{Align, Placement, Positioner, TestSupportExt};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    Anchor, Animation, AnimationExt, AnyElement, App, Bounds, Context, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, Role, SharedString, StatefulInteractiveElement,
    Styled, Toggled, Window, canvas, div, point, prelude::FluentBuilder, px, size,
};

impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let motion = &cx.theme().motion;
        let (duration, easing) = (motion.duration_fast, motion.easing_enter.clone());
        let travel = f32::from(motion.distance_short.to_pixels(window.rem_size()));
        let ring = cx.theme().foreground.alpha(LINE_INK);
        let has_leading = self.items.iter().any(|item| item.leading.is_some());
        let rows: Vec<AnyElement> = self
            .items
            .iter()
            .enumerate()
            .map(|(ix, item)| self.row(ix, None, item, has_leading, cx))
            .collect();
        // Fades in while sliding down onto its place; the shadow ramps up late, as it would
        // otherwise show through the half-transparent surface.
        let menu = panel(div().id("menu").test_support(), cx)
            .role(Role::Menu)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .children(rows)
            .children(self.render_submenu(window, cx))
            .with_animation(
                "menu-open",
                Animation::new(duration).with_easing(move |delta| easing.sample(delta)),
                move |menu, delta| {
                    menu.top(px(-travel * (1. - delta)))
                        .opacity(delta)
                        .shadow(shadow(ring, delta.powi(3)))
                },
            );
        // Over the whole window, so a press outside the menu closes it instead of reaching
        // what is underneath.
        div()
            .id("menu-backdrop")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .on_any_mouse_down(cx.listener(|menu, _, window, cx| menu.close(window, cx)))
            .child(
                Positioner::corner(Anchor::TopLeft, self.position)
                    .margin(px(WINDOW_MARGIN))
                    .occlude()
                    .child(menu),
            )
    }
}

impl Menu {
    /// The open submenu beside its row, flipped to the left when the window has no room on the
    /// right; `None` until the row has been drawn once.
    fn render_submenu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let parent = self.submenu.as_ref()?.parent;
        let items = self.items.get(parent)?.submenu_items()?;
        let trigger = beside(self.row_bounds.borrow().get(parent).copied()?)?;
        let has_leading = items.iter().any(|item| item.leading.is_some());
        let ring = cx.theme().foreground.alpha(LINE_INK);
        let rows: Vec<AnyElement> = items
            .iter()
            .enumerate()
            .map(|(ix, item)| self.row(ix, Some(parent), item, has_leading, cx))
            .collect();
        Some(
            Positioner::side(trigger)
                .placement(Placement::Right)
                .align(Align::Start)
                .offset(px(-SUBMENU_OVERLAP))
                .margin(px(WINDOW_MARGIN))
                .occlude()
                .child(
                    panel(div().id("menu-submenu").test_support(), cx)
                        .role(Role::Menu)
                        .shadow(shadow(ring, 1.))
                        .on_hover(cx.listener(|menu, hovered: &bool, _, cx| {
                            menu.hover_submenu(None, *hovered, cx)
                        }))
                        .children(rows)
                        .children(self.render_panel(window, cx)),
                ),
        )
    }

    /// The open panel beside its submenu row, flipped to the left when the window has no room
    /// on the right; `None` until the row has been drawn once.
    fn render_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let submenu = self.submenu.as_ref()?;
        let item = self
            .items
            .get(submenu.parent)?
            .submenu_items()?
            .get(submenu.panel?)?;
        let Kind::Panel { content, .. } = &item.kind else {
            return None;
        };
        let trigger = beside(self.panel_row.get())?;
        let ring = cx.theme().foreground.alpha(LINE_INK);
        Some(
            Positioner::side(trigger)
                .placement(Placement::Right)
                .align(Align::Start)
                .offset(px(-SUBMENU_OVERLAP))
                .margin(px(WINDOW_MARGIN))
                .occlude()
                .child(
                    panel(div().id("menu-panel").test_support(), cx)
                        .role(Role::Group)
                        .aria_label(item.label.clone())
                        .shadow(shadow(ring, 1.))
                        .on_hover(
                            cx.listener(|menu, hovered: &bool, _, cx| {
                                menu.hover_panel(*hovered, cx)
                            }),
                        )
                        // Keys typed into the panel stay in it; Escape still closes it.
                        .on_key_down(|event, _, cx| {
                            if matches!(
                                event.keystroke.key.as_str(),
                                "up" | "down" | "left" | "right" | "enter"
                            ) {
                                cx.stop_propagation();
                            }
                        })
                        .child(content(window, cx)),
                ),
        )
    }

    /// Row `ix` of the menu, or of the submenu of row `parent`.
    fn row(
        &self,
        ix: usize,
        parent: Option<usize>,
        item: &MenuItem,
        has_leading: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (line, muted, foreground) = (
            theme.foreground.alpha(LINE_INK),
            theme.muted_foreground,
            theme.popover_foreground,
        );
        let (highlight, radius) = (theme.foreground.alpha(HIGHLIGHT_INK), theme.radius);
        if matches!(item.kind, Kind::Separator) {
            return div()
                .flex_shrink_0()
                .h(px(1.))
                .my(px(SEPARATOR_MARGIN))
                .bg(line)
                .into_any_element();
        }
        let submenu = self.submenu.as_ref();
        let (highlighted, active) = match parent {
            None => {
                let highlighted = self.highlight == Some(ix);
                let keyboard_in_submenu = submenu.is_some_and(|open| open.highlight.is_some());
                (highlighted, highlighted && !keyboard_in_submenu)
            }
            Some(_) => {
                let highlighted = submenu.is_some_and(|open| open.highlight == Some(ix));
                (highlighted, highlighted)
            }
        };
        let opens_submenu = parent.is_none() && item.submenu_items().is_some();
        let opens_panel = parent.is_some() && item.opens_panel();
        let expanded = (opens_submenu && submenu.is_some_and(|open| open.parent == ix))
            || (opens_panel && submenu.is_some_and(|open| open.panel == Some(ix)));
        let leading = match &item.leading {
            Some(Leading::Icon(name)) => Some(
                app_icon(name)
                    .size(px(ICON_SIZE))
                    .flex_shrink_0()
                    .text_color(muted)
                    .into_any_element(),
            ),
            Some(Leading::Element(element)) => Some(element(cx)),
            None if has_leading => {
                Some(div().flex_shrink_0().size(px(ICON_SIZE)).into_any_element())
            }
            None => None,
        };
        let check = (item.checked == Some(true)).then(|| app_icon("check").text_color(foreground));
        let chevron =
            (opens_submenu || opens_panel).then(|| app_icon("chevron-right").text_color(muted));
        let row = div()
            .id(SharedString::from(format!("menu-item-{}", item.id)))
            .test_support()
            .role(if item.checked.is_some() {
                Role::MenuItemCheckBox
            } else {
                Role::MenuItem
            })
            .aria_label(item.label.clone())
            .aria_disabled(item.disabled)
            .when_some(item.checked, |row, checked| {
                row.aria_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
            })
            .when(opens_submenu || opens_panel, |row| {
                row.aria_expanded(expanded)
            })
            .when(active, |row| row.aria_active_descendant())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(ICON_GAP))
            .h(px(ROW_HEIGHT))
            .px(px(ROW_PADDING))
            .rounded(radius)
            .when(highlighted, |row| row.bg(highlight))
            .when(item.disabled, |row| row.opacity(DISABLED_OPACITY))
            .children(leading)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(item.label.clone()),
            )
            .children(
                check
                    .into_iter()
                    .chain(chevron)
                    .map(|icon| icon.size(px(TRAILING_SIZE)).flex_shrink_0()),
            );
        // A disabled row in the menu still lets the open submenu close.
        let row = row.when(parent.is_none() || !item.disabled, |row| {
            row.on_hover(
                cx.listener(move |menu, hovered: &bool, _, cx| match parent {
                    None => menu.hover_row(ix, *hovered, cx),
                    Some(_) => menu.hover_submenu(Some(ix), *hovered, cx),
                }),
            )
        });
        if item.disabled {
            return row.into_any_element();
        }
        let (bounds, panel_row) = (self.row_bounds.clone(), self.panel_row.clone());
        row.cursor_pointer()
            .on_click(cx.listener(move |menu, _, window, cx| match parent {
                None => menu.choose(ix, None, false, window, cx),
                Some(parent) => menu.choose(parent, Some(ix), false, window, cx),
            }))
            .when(opens_submenu, |row| {
                // Pinned to every edge: an unpinned absolute child starts inside the row's padding.
                row.child(
                    canvas(
                        move |row, _, _| {
                            if let Some(slot) = bounds.borrow_mut().get_mut(ix) {
                                *slot = row;
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
            })
            .when(opens_panel, |row| {
                row.child(
                    canvas(move |row, _, _| panel_row.set(row), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
            })
            .into_any_element()
    }
}

/// Where a submenu or panel opens beside the row drawn at `row`: measured from the edge of the
/// surface the row is on, so it overlaps that edge and its first row lines up with the row.
/// `None` until the row has been drawn once.
fn beside(row: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    if row.size.width <= px(0.) {
        return None;
    }
    let inset = px(PADDING);
    Some(Bounds::new(
        point(row.left() - inset, row.top() - inset),
        size(row.size.width + inset * 2., row.size.height),
    ))
}

/// The surface shared by a menu, its submenu, and a panel.
fn panel<E: Styled>(panel: E, cx: &App) -> E {
    let theme = cx.theme();
    panel
        .flex()
        .flex_col()
        .min_w(px(MIN_WIDTH))
        .p(px(PADDING))
        .rounded(px(SURFACE_RADIUS))
        .bg(surface(theme))
        .text_size(px(LABEL_SIZE))
        .text_color(theme.popover_foreground)
}
