//! The sidebar's Calendars section: one row per Account, or the step that grants access.

use std::rc::Rc;

use super::NativeView;
use super::controls::app_icon;
use super::sidebar_row::{activate, row};
use crate::settings::{self, SidebarSection};
use crumble_eventkit::{Access, Account, Provider};
use crumble_theme::sidebar::{DETAIL_SIZE, ICON_GAP, ROW_GAP, ROW_PADDING};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, ParentElement, Stateful,
    Styled, Window, div, prelude::FluentBuilder, px,
};

const PRIVACY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars";
const INTERNET_ACCOUNTS: &str =
    "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

/// A Disabled Account's row is dimmed to this opacity.
const DISABLED_OPACITY: f32 = 0.5;
/// Muted copy and menu icons sit a step below their label, as in SkillBinder.
const MUTED_OPACITY: f32 = 0.65;

/// What the Calendars section shows.
#[derive(Debug, PartialEq, Eq)]
enum Section {
    /// No EventKit on this platform.
    Hidden,
    /// One row that asks for access.
    Connect,
    /// One row that opens Privacy settings, the only place a refusal can be undone.
    AllowAccess,
    /// A hint that opens Internet Accounts settings.
    AddAccounts,
    /// One row per Account; none until the first read finishes.
    Accounts,
}

fn section(access: Access, loaded: bool, accounts: usize) -> Section {
    match access {
        Access::Unsupported => Section::Hidden,
        Access::NotDetermined => Section::Connect,
        Access::Denied => Section::AllowAccess,
        Access::Granted if loaded && accounts == 0 => Section::AddAccounts,
        Access::Granted => Section::Accounts,
    }
}

/// Hugeicons has no Outlook mark, so Outlook shows Microsoft's.
pub(super) fn provider_icon(provider: Provider) -> Icon {
    app_icon(match provider {
        Provider::Google => "google",
        Provider::Outlook => "microsoft",
        Provider::Apple => "apple",
        Provider::Other => "calendar",
    })
}

impl NativeView {
    /// The section title, which folds the rows, and the rows; `None` without EventKit.
    pub(super) fn render_calendars_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sync = self.calendar_sync.read(cx);
        let rows: Vec<AnyElement> =
            match section(sync.access(), sync.loaded(), sync.accounts().len()) {
                Section::Hidden => return None,
                Section::Connect => vec![
                    activate(
                        row(
                            "connect-calendars",
                            app_icon("calendar"),
                            "Connect calendars",
                            false,
                            cx,
                        ),
                        "Connect calendars",
                        cx,
                        |view, _, cx| view.calendar_sync.read(cx).request_access(),
                    )
                    .into_any_element(),
                ],
                Section::AllowAccess => vec![
                    activate(
                        row(
                            "allow-calendar-access",
                            app_icon("calendar"),
                            "Allow calendar access",
                            false,
                            cx,
                        ),
                        "Allow calendar access",
                        cx,
                        |_, _, cx| cx.open_url(PRIVACY_SETTINGS),
                    )
                    .into_any_element(),
                ],
                Section::AddAccounts => vec![self.add_accounts_hint(cx).into_any_element()],
                Section::Accounts => sync
                    .accounts()
                    .iter()
                    .map(|account| self.account_row(account, cx))
                    .collect(),
            };
        let items = rows.len().max(1);
        Some(
            div()
                .flex()
                .flex_col()
                .child(self.section_header(SidebarSection::Calendars, "Calendars", cx))
                .children(
                    self.section_rows(
                        SidebarSection::Calendars,
                        items,
                        div()
                            .id("sidebar-calendars")
                            .flex()
                            .flex_col()
                            .gap(px(ROW_GAP))
                            .children(rows),
                    ),
                )
                .into_any_element(),
        )
    }

    fn add_accounts_hint(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        const HINT: &str = "Add accounts in System Settings › Internet Accounts";
        let theme = cx.theme();
        let (muted, hover) = (
            theme.muted_foreground.opacity(MUTED_OPACITY),
            theme.foreground,
        );
        activate(
            div()
                .id("add-accounts")
                .px(px(ROW_PADDING))
                .py_1()
                .cursor_pointer()
                .text_size(px(DETAIL_SIZE))
                .text_color(muted)
                .hover(move |style| style.text_color(hover))
                .child(HINT),
            HINT,
            cx,
            |_, _, cx| cx.open_url(INTERNET_ACCOUNTS),
        )
    }

    /// Clicking the row does nothing; right-clicking it opens a menu that Disables or Enables
    /// the Account.
    fn account_row(&self, account: &Account, cx: &Context<Self>) -> AnyElement {
        let disabled = self.settings.disabled_accounts.contains(&account.id);
        let (view, id) = (cx.entity().downgrade(), account.id.clone());
        let toggle = move |cx: &mut App| {
            let _ = view.update(cx, |view, cx| view.toggle_account(&id, cx));
        };
        row(
            format!("account-{}", account.id),
            provider_icon(account.provider),
            account.name.clone(),
            false,
            cx,
        )
        .cursor_default()
        .when(disabled, |row| row.opacity(DISABLED_OPACITY))
        .context_menu(account_menu(disabled, toggle))
        .into_any_element()
    }

    pub(super) fn toggle_account(&mut self, id: &str, cx: &mut Context<Self>) {
        settings::toggle(&mut self.settings.disabled_accounts, id);
        self.settings.save();
        cx.notify();
    }

    pub(super) fn toggle_calendar(&mut self, id: &str, cx: &mut Context<Self>) {
        settings::toggle(&mut self.settings.disabled_calendars, id);
        self.settings.save();
        cx.notify();
    }
}

/// An Account row's context menu; its one item runs `toggle`.
fn account_menu(
    disabled: bool,
    toggle: impl Fn(&mut App) + 'static,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let toggle = Rc::new(toggle);
    move |menu, _, cx| {
        let toggle = toggle.clone();
        let (label, icon) = if disabled {
            ("Enable", "view")
        } else {
            ("Disable", "view-off")
        };
        let color = cx.theme().muted_foreground.opacity(MUTED_OPACITY);
        menu.item(
            PopupMenuItem::new(label)
                .icon(app_icon(icon).text_color(color).mr(px(ICON_GAP - 4.0)))
                .on_click(move |_, _, cx| toggle(cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Access, Account, Provider, Section, account_menu, section};
    use gpui_kit::component::menu::ContextMenuExt;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };
    use std::rc::Rc;

    #[test]
    fn each_access_state_picks_its_rows() {
        assert_eq!(section(Access::Unsupported, false, 0), Section::Hidden);
        assert_eq!(section(Access::NotDetermined, false, 0), Section::Connect);
        assert_eq!(section(Access::Denied, true, 2), Section::AllowAccess);
        assert_eq!(section(Access::Granted, true, 0), Section::AddAccounts);
        assert_eq!(section(Access::Granted, true, 2), Section::Accounts);
        // No hint flashes before the first read finishes.
        assert_eq!(section(Access::Granted, false, 0), Section::Accounts);
    }

    /// Stands in for the sidebar: one Account row whose context menu counts its toggles.
    struct Sidebar {
        account: Account,
        toggles: Rc<std::cell::Cell<usize>>,
    }

    impl Render for Sidebar {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let toggles = self.toggles.clone();
            div()
                .id("sidebar")
                .size_full()
                .child(
                    // Fills the window: wrapped in its context menu, the row is no longer
                    // observable by ID, so the test right-clicks the window's centre.
                    div()
                        .id("row")
                        .size_full()
                        .child(self.account.name.clone())
                        .context_menu(account_menu(false, move |_| toggles.set(toggles.get() + 1))),
                )
                .test_support()
        }
    }

    #[gpui_kit::test]
    fn right_clicking_an_account_disables_it(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let toggles = Rc::new(std::cell::Cell::new(0));
        let account = Account {
            id: "source-1".into(),
            name: "Work".into(),
            provider: Provider::Google,
        };
        let sidebar_toggles = toggles.clone();
        let (handle, _) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            Default::default(),
                            size(px(400.), px(300.)),
                        ))),
                        ..Default::default()
                    },
                    cx,
                    |_, cx| {
                        cx.new(|_| Sidebar {
                            account,
                            toggles: sidebar_toggles,
                        })
                    },
                )
            })
            .unwrap_or_else(|error| panic!("open test window: {error}"));

        let step = |cx: &mut TestAppContext, act: &dyn Fn(&mut Window, &mut gpui_kit::App)| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                act(window, cx);
            })
            .unwrap_or_else(|error| panic!("update test window: {error}"));
            // The menu is built in a deferred callback.
            cx.run_until_parked();
        };
        step(cx, &|window, cx| window.right_click("sidebar", cx));
        step(cx, &|window, cx| window.click(0usize, cx));

        assert_eq!(toggles.get(), 1);
    }
}
