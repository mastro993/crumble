//! Account Aliases: the Alias field of each Account in Settings, and the sidebar's Rename.

use std::collections::HashMap;

use super::NativeView;
use crumble_store::account_label;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext, Context, Entity, Focusable, KeyDownEvent, Subscription, Window};

/// The Account row being renamed in the sidebar.
pub(super) struct Rename {
    pub(super) id: String,
    pub(super) input: Entity<InputState>,
    _subscription: Subscription,
}

/// The Settings Alias field of each Account, by identifier.
#[derive(Default)]
pub(super) struct AliasFields(HashMap<String, (Entity<InputState>, Subscription)>);

impl AliasFields {
    pub(super) fn get(&self, id: &str) -> Option<&Entity<InputState>> {
        self.0.get(id).map(|(input, _)| input)
    }
}

impl NativeView {
    /// Gives each Account an Alias field and drops the fields of Accounts that are gone.
    pub(super) fn sync_alias_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .calendar_sync
            .read(cx)
            .accounts()
            .iter()
            .map(|account| account.id.clone())
            .collect();
        self.alias_fields.0.retain(|id, _| ids.contains(id));
        for id in ids {
            if self.alias_fields.0.contains_key(&id) {
                continue;
            }
            let alias = self.store.account_alias(&id).unwrap_or_default().to_owned();
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Alias")
                    .default_value(alias)
            });
            let account = id.clone();
            let subscription = cx.subscribe_in(&input, window, move |view, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let alias = input.read(cx).value();
                    view.save_alias(&account, &alias, cx);
                }
            });
            self.alias_fields.0.insert(id, (input, subscription));
        }
    }

    /// Turns the Account row `id` into a field holding its name, all selected.
    pub(super) fn start_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let sync = self.calendar_sync.read(cx);
        let Some(account) = sync.accounts().iter().find(|account| account.id == id) else {
            return;
        };
        let name = self.store.account_name(account).to_owned();
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).default_value(name);
            input.select_all(window, cx);
            input
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe_in(&input, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                view.finish_rename(true, window, cx);
            }
        });
        self.renaming = Some(Rename {
            id: id.to_owned(),
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Ends the Rename, saving the typed name when `save` is set.
    pub(super) fn finish_rename(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        let focused = rename.input.focus_handle(cx).is_focused(window);
        let typed = rename.input.read(cx).value();
        // Enter and Escape leave focus in the field; give it back to the window for its shortcuts.
        if focused {
            window.focus(&self.focus, cx);
        }
        if save {
            self.save_alias(&rename.id, &typed, cx);
            let alias = self
                .store
                .account_alias(&rename.id)
                .unwrap_or_default()
                .to_owned();
            if let Some(field) = self.alias_fields.get(&rename.id) {
                field.update(cx, |field, cx| field.set_value(alias, window, cx));
            }
        }
        cx.notify();
    }

    /// Escape ends the Rename without saving.
    pub(super) fn rename_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" {
            self.finish_rename(false, window, cx);
            cx.stop_propagation();
        }
    }

    /// Saves `alias` for the Account `id`; a blank one, or the Account's own name, removes it.
    fn save_alias(&mut self, id: &str, alias: &str, cx: &mut Context<Self>) {
        let sync = self.calendar_sync.read(cx);
        let Some(account) = sync.accounts().iter().find(|account| account.id == id) else {
            return;
        };
        self.store
            .set_account_alias(id, alias, account_label(account));
        cx.notify();
    }
}
