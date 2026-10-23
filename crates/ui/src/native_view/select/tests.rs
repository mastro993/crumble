//! UI integration tests of a searchable select whose choices can be any text, as a Provider's
//! Model select is: a choice typed in, a row picked after the choices are listed again, the
//! menu's width as a search filters it, and the picked choice's logo.

use super::{Choice, Leading, Picked, Select};
use gpui_kit::base::TestSupportExt;
use gpui_kit::test::{ElementSnapshot, TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, Styled, Subscription, TestAppContext, Window,
    WindowBounds, WindowOptions, div, px, size,
};
use std::rc::Rc;

/// A Model's id, listed or typed.
#[derive(Clone)]
struct Model(String);

impl Choice for Model {
    type Value = String;

    fn title(&self) -> SharedString {
        self.0.clone().into()
    }

    fn value(&self) -> &String {
        &self.0
    }

    fn typed() -> Option<fn(SharedString) -> Self> {
        Some(|model| Self(model.into()))
    }

    /// A logo for the Models of one family.
    fn leading(&self) -> Option<Leading> {
        self.0.starts_with("gpt").then(|| {
            Rc::new(|_: &App| {
                div()
                    .id("gpt-logo")
                    .test_support()
                    .size(px(16.))
                    .into_any_element()
            }) as Leading
        })
    }
}

fn models(ids: &[&str]) -> Vec<Model> {
    ids.iter().map(|id| Model((*id).to_owned())).collect()
}

/// Draws the select and keeps what it picks, in order.
struct Owner {
    select: Entity<Select<Model>>,
    picks: Vec<String>,
    _subscription: Subscription,
}

impl Owner {
    fn new(ids: &[&str], selected: Option<usize>, cx: &mut Context<Self>) -> Self {
        let items = models(ids);
        let select = cx.new(|cx| Select::new("Model", items, selected, cx).searchable());
        let subscription = cx.subscribe(&select, |owner, _, Picked(model): &Picked<String>, _| {
            owner.picks.push(model.clone())
        });
        Self {
            select,
            picks: Vec::new(),
            _subscription: subscription,
        }
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.select.clone())
    }
}

struct Harness {
    handle: AnyWindowHandle,
    owner: Entity<Owner>,
}

impl Harness {
    /// Opens a window with a select listing `ids`, row `selected` picked.
    fn open(
        cx: &mut TestAppContext,
        ids: &'static [&'static str],
        selected: Option<usize>,
    ) -> Self {
        cx.update(gpui_kit::init);
        let (handle, owner) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(600.), px(600.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| cx.new(|cx| Owner::new(ids, selected, cx)),
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));
        let harness = Self { handle, owner };
        // Focus events only reach an active window.
        harness.step(cx, |window, _| window.activate_window());
        harness
    }

    /// Draws a frame and runs `act`, then draws the result.
    fn step(&self, cx: &mut TestAppContext, act: impl FnOnce(&mut Window, &mut App)) {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            act(window, cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
        self.check(cx, |_| {});
    }

    fn check(&self, cx: &mut TestAppContext, check: impl FnOnce(&mut Window)) {
        cx.update_window(self.handle, |_, window, cx| {
            window.render_frame(cx);
            check(window);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));
    }

    fn menu_open(&self, cx: &mut TestAppContext) -> bool {
        let mut open = false;
        self.check(cx, |window| open = window.try_find("menu").is_some());
        open
    }

    /// Opens the select and types `text` into its search field.
    fn search(&self, cx: &mut TestAppContext, text: &str) {
        self.step(cx, |window, cx| window.click("select", cx));
        self.step(cx, |window, cx| window.input(text, cx));
    }

    /// The menu row labelled `label`, if one shows.
    fn row(&self, cx: &mut TestAppContext, label: &str) -> Option<ElementSnapshot> {
        let mut row = None;
        self.check(cx, |window| {
            row = window.try_find_by_label(label).filter(|row| row.visible());
        });
        row
    }

    /// Clicks the menu row labelled `label`.
    fn click_row(&self, cx: &mut TestAppContext, label: &str) {
        let id = self
            .row(cx, label)
            .and_then(|row| row.path().last().cloned());
        let id = id.unwrap_or_else(|| panic!("no row shows {label}"));
        self.step(cx, |window, cx| window.click(id, cx));
    }

    fn menu_width(&self, cx: &mut TestAppContext) -> Pixels {
        let mut width = px(0.);
        self.check(cx, |window| width = window.find("menu").bounds().size.width);
        width
    }

    fn picks(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| self.owner.read(cx).picks.clone())
    }

    fn picked(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.update(|cx| {
            let select = self.owner.read(cx).select.read(cx);
            select.selected_value().cloned()
        })
    }
}

#[gpui_kit::test]
fn a_choice_the_select_does_not_list_is_typed_and_picked(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, &["gpt-6-luna", "gpt-6.1-sol"], Some(0));
    // A choice the select lists, whatever the case, is picked from its row.
    harness.search(cx, "GPT-6-LUNA");
    assert!(harness.row(cx, "gpt-6-luna").is_some());
    assert!(harness.row(cx, "Use “GPT-6-LUNA”").is_none());
    harness.step(cx, |window, cx| window.press("escape", cx));

    // Clicked, the row offering a new id picks it as typed, trimmed.
    harness.search(cx, "  My-Fine-Tune ");
    harness.click_row(cx, "Use “My-Fine-Tune”");
    assert!(!harness.menu_open(cx));
    assert_eq!(harness.picked(cx).as_deref(), Some("My-Fine-Tune"));

    // Enter chooses it once it is the only row left.
    harness.search(cx, "whisper-large-v3");
    harness.step(cx, |window, cx| window.press("enter", cx));
    assert!(!harness.menu_open(cx));
    assert_eq!(harness.picks(cx), ["My-Fine-Tune", "whisper-large-v3"]);

    // Opened again, the select lists what was typed and checks the last pick.
    harness.step(cx, |window, cx| window.click("select", cx));
    assert!(harness.row(cx, "My-Fine-Tune").is_some());
    let row = harness.row(cx, "whisper-large-v3");
    assert_eq!(row.and_then(|row| row.checked()), Some(true));
}

#[gpui_kit::test]
fn a_row_picks_the_choice_it_shows_after_the_choices_are_listed_again(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, &["gpt-6-luna", "my-model"], Some(1));
    harness.step(cx, |window, cx| window.click("select", cx));
    // A Provider lists a Model while the menu is open, which moves the picked one down a row.
    harness.step(cx, |_, cx| {
        let select = harness.owner.read(cx).select.clone();
        select.update(cx, |select, cx| {
            let items = models(&["gpt-6-luna", "gpt-7", "my-model"]);
            select.set_items(items, Some(2), cx)
        });
    });

    harness.click_row(cx, "my-model");
    assert_eq!(harness.picks(cx), ["my-model"]);
}

#[gpui_kit::test]
fn a_search_keeps_the_menu_as_wide_as_it_opened(cx: &mut TestAppContext) {
    let harness = Harness::open(
        cx,
        &["gpt-6", "a-model-named-longer-than-the-menu-is-wide"],
        None,
    );
    harness.step(cx, |window, cx| window.click("select", cx));
    let opened = harness.menu_width(cx);
    // The long row is hidden.
    harness.step(cx, |window, cx| window.input("gpt", cx));
    assert_eq!(harness.menu_width(cx), opened);
    // The row offering a text longer than any row cuts it short.
    let text = "-fine-tuned-on-every-meeting-note-and-transcript-kept-this-year";
    harness.step(cx, |window, cx| window.input(text, cx));
    assert!(harness.row(cx, &format!("Use “gpt{text}”")).is_some());
    assert_eq!(harness.menu_width(cx), opened);
}

#[gpui_kit::test]
fn a_choice_s_logo_leads_its_row_and_the_select_once_picked(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, &["llama3.2", "gpt-6-luna"], Some(0));
    // How many logos show in the menu, and in the select itself.
    let logos = |harness: &Harness, cx: &mut TestAppContext| {
        let mut shown = (0, 0);
        harness.check(cx, |window| {
            for logo in window.find_all("gpt-logo") {
                if logo.path().iter().any(|id| id.to_string() == "menu") {
                    shown.0 += 1;
                } else {
                    shown.1 += 1;
                }
            }
        });
        shown
    };
    assert_eq!(logos(&harness, cx), (0, 0));
    // Only the row whose choice has a logo leads with it.
    harness.step(cx, |window, cx| window.click("select", cx));
    assert_eq!(logos(&harness, cx), (1, 0));
    harness.click_row(cx, "gpt-6-luna");
    assert_eq!(logos(&harness, cx), (0, 1));
}
