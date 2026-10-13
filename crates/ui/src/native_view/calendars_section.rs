//! The sidebar's Calendars section: one row per Account, or the step that grants access.

use std::rc::Rc;

use super::NativeView;
use super::controls::app_icon;
use super::sidebar_row::{activate, row};
use crate::settings;
use crumble_eventkit::{Access, Account, Provider};
use crumble_theme::sidebar::{ROW_GAP, ROW_PADDING};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, StyledExt, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, Styled, div, prelude::FluentBuilder, px,
};

const PRIVACY_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars";
const INTERNET_ACCOUNTS: &str =
    "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

/// A Disabled Account's row is dimmed to this opacity.
const DISABLED_OPACITY: f32 = 0.5;

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
    pub(super) fn render_calendars_section(&self, cx: &mut Context<Self>) -> Option<Div> {
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
                    .map(|account| self.account_row(account, cx).into_any_element())
                    .collect(),
            };
        Some(
            v_flex()
                .gap(px(ROW_GAP))
                .pt_4()
                .child(
                    div()
                        .px(px(ROW_PADDING))
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Calendars"),
                )
                .children(rows),
        )
    }

    fn add_accounts_hint(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        const HINT: &str = "Add accounts in System Settings › Internet Accounts";
        let hover = cx.theme().foreground;
        activate(
            div()
                .id("add-accounts")
                .px(px(ROW_PADDING))
                .py_1()
                .cursor_pointer()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .hover(move |style| style.text_color(hover))
                .child(HINT),
            HINT,
            cx,
            |_, _, cx| cx.open_url(INTERNET_ACCOUNTS),
        )
    }

    /// The row itself does nothing; its ⋯ menu, shown on hover, Disables or Enables the Account.
    fn account_row(&self, account: &Account, cx: &Context<Self>) -> Stateful<Div> {
        let disabled = self.settings.disabled_accounts.contains(&account.id);
        let group = SharedString::from(format!("account-{}", account.id));
        let view = cx.entity().downgrade();
        let id = account.id.clone();
        let menu = account_menu(account, disabled, group.clone(), move |cx| {
            let _ = view.update(cx, |view, cx| view.toggle_account(&id, cx));
        });
        row(
            group.clone(),
            provider_icon(account.provider),
            account.name.clone(),
            false,
            cx,
        )
        .group(group)
        .cursor_default()
        .when(disabled, |row| row.opacity(DISABLED_OPACITY))
        .child(menu)
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

/// The ⋯ button of an Account row, shown while `group` is hovered; its one item runs `toggle`.
fn account_menu(
    account: &Account,
    disabled: bool,
    group: SharedString,
    toggle: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    let toggle = Rc::new(toggle);
    // Not `controls::button`: it stops the mouse-down that opens the menu.
    Button::new(SharedString::from(format!("account-menu-{}", account.id)))
        .cursor_pointer()
        .ghost()
        .xsmall()
        .icon(app_icon("more").size_4())
        .accessibility_label(format!("{} actions", account.name))
        // Transparent rather than invisible, so Tab still reaches it.
        .opacity(0.)
        .group_hover(group, |style| style.opacity(1.))
        .focus_visible(|style| style.opacity(1.))
        .dropdown_menu(move |menu, _, _| {
            let toggle = toggle.clone();
            menu.item(
                PopupMenuItem::new(if disabled { "Enable" } else { "Disable" })
                    .on_click(move |_, _, cx| toggle(cx)),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{Access, Account, Provider, Section, account_menu, section};
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

    /// Stands in for the sidebar: one Account row whose menu counts its toggles.
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
                    div()
                        .id("row")
                        .group("row")
                        .w(px(240.))
                        .child(self.account.name.clone())
                        .child(account_menu(
                            &self.account,
                            false,
                            "row".into(),
                            move |_| toggles.set(toggles.get() + 1),
                        )),
                )
                .test_support()
        }
    }

    #[gpui_kit::test]
    fn the_account_menu_disables_the_account(cx: &mut TestAppContext) {
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

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("account-menu-source-1", cx);
            window.render_frame(cx);
            window.click(0usize, cx);
            window.render_frame(cx);
        })
        .unwrap_or_else(|error| panic!("update test window: {error}"));

        assert_eq!(toggles.get(), 1);
    }
}
