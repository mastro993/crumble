//! The Accounts and Calendars read from EventKit, kept current while the app runs.

use crumble_eventkit::{self as eventkit, Access, Account, Calendar, ChangeObserver};
use gpui_kit::{AppContext, Context, Task};

/// What Crumble knows of the user's Accounts and Calendars.
///
/// It reads them at startup only when access is already granted, so launching never prompts,
/// and again whenever EventKit reports a change. A failed read is logged and the last data stays.
pub(crate) struct CalendarSync {
    access: Access,
    accounts: Vec<Account>,
    calendars: Vec<Calendar>,
    /// Whether a read has finished since access was granted.
    loaded: bool,
    /// Wakes the reload loop; EventKit calls back on threads of its own.
    changed: async_channel::Sender<()>,
    _observer: ChangeObserver,
    _watch: Task<()>,
    /// The read in flight; replacing it drops a stale one.
    read: Task<()>,
}

impl CalendarSync {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let (changed, receiver) = async_channel::unbounded();
        let notify = changed.clone();
        let observer = eventkit::observe_changes(move || {
            let _ = notify.try_send(());
        });
        let watch = cx.spawn(async move |this, cx| {
            while receiver.recv().await.is_ok() {
                // EventKit posts changes in bursts; one reload covers them all.
                while receiver.try_recv().is_ok() {}
                if this.update(cx, |sync, cx| sync.reload(cx)).is_err() {
                    break;
                }
            }
        });
        let mut sync = Self {
            access: Access::NotDetermined,
            accounts: Vec::new(),
            calendars: Vec::new(),
            loaded: false,
            changed,
            _observer: observer,
            _watch: watch,
            read: Task::ready(()),
        };
        sync.reload(cx);
        sync
    }

    pub(crate) fn access(&self) -> Access {
        self.access
    }

    /// Every Account, sorted by name.
    pub(crate) fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    /// Every Calendar of every Account, sorted by name.
    pub(crate) fn calendars(&self) -> &[Calendar] {
        &self.calendars
    }

    pub(crate) fn loaded(&self) -> bool {
        self.loaded
    }

    /// Shows the macOS permission prompt, then reads again whatever the user chose.
    pub(crate) fn request_access(&self) {
        let changed = self.changed.clone();
        eventkit::request_access(move |_| {
            let _ = changed.try_send(());
        });
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.access = eventkit::access();
        cx.notify();
        if self.access != Access::Granted {
            return;
        }
        // Apple advises reading off the main thread.
        let read = cx.background_spawn(async {
            anyhow::Ok((eventkit::accounts()?, eventkit::calendars()?))
        });
        self.read = cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |sync, cx| match result {
                Ok((accounts, calendars)) => {
                    sync.accounts = accounts;
                    sync.calendars = calendars;
                    sync.loaded = true;
                    cx.notify();
                }
                Err(error) => eprintln!("Could not read calendars: {error}"),
            });
        });
    }
}
