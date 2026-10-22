//! Settings › Local Models: Handy's list of Models Crumble can download and run on the Mac,
//! filtered by name, streaming and language and sorted, with each one's download, Use and Delete
//! (ADR 0010). `crumble-local-models` does the downloading and deleting off the main thread;
//! this holds what shows. The page is drawn in `render`, each Model's card in `card`; only the
//! rows in view are drawn, as drawing every card each frame made scrolling slow.

mod card;
mod render;

use super::NativeView;
use super::select::{Choice, Picked, Select};
use card::{Card, Place};
use crumble_local_models::{Downloaded, Library, Model, Outcome, catalog, find};
use crumble_remote::Provider;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AppContext, Context, Entity, ListAlignment, ListOffset, ListState, SharedString, Subscription,
    Task, Window, px,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What the Local Models section shows, and the downloads running.
pub(super) struct LocalModels {
    /// The models folder; `None` when this platform has no data folder.
    library: Option<Library>,
    /// The downloaded Models as last read.
    downloaded: Vec<Downloaded>,
    /// Whether the models folder is read again on the next frame of Settings: when Settings
    /// opens, a section is picked, or a download ends, as Finder may have changed it too.
    pub(super) reread: bool,
    /// The downloads running, by slug.
    downloads: HashMap<String, Download>,
    /// Why each Model's last download failed, until it is tried again.
    failures: HashMap<String, String>,
    search: Entity<InputState>,
    /// Whether only Models that can stream show.
    streaming: bool,
    /// The language filter, made the first time the section shows.
    language: Option<Entity<Select<LanguageFilter>>>,
    sort: Entity<Select<Sort>>,
    /// The picked order, and whether it runs from the most down; picking it again flips it.
    sorted: (Sort, bool),
    /// The rows the list shows, as last drawn.
    rows: Rc<Vec<Row>>,
    list: ListState,
    _subscriptions: Vec<Subscription>,
}

struct Download {
    held: u64,
    total: u64,
    /// Set by Cancel; the download stops at its next read.
    cancel: Arc<AtomicBool>,
    /// Receives the download's progress; dropping it drops its result.
    _updates: Task<()>,
}

/// A row of the section's list.
#[derive(Clone, PartialEq)]
enum Row {
    /// The section's title, description, search field and filters.
    Header,
    /// A group's heading.
    Label(&'static str),
    /// Apple's built-in Model, at the top of the Downloaded card.
    Apple(Place),
    Card(Card, Place),
    /// The line in place of the cards when no Model matches.
    Nothing,
    /// Where the list and its scores come from.
    Credit,
}

/// What each group's Models are ordered by.
#[derive(Clone, Copy, PartialEq)]
enum Sort {
    /// Handy's recommended Models, in its order, then the rest in the list's.
    Recommended,
    Accuracy,
    Speed,
    Size,
}

impl Sort {
    /// Whether this order first runs from the most down: all but Size, which puts the smallest
    /// first.
    fn descending(self) -> bool {
        self != Sort::Size
    }

    /// Puts `models` in this order, from the most down when `descending`; Models that tie keep
    /// theirs.
    fn apply<T>(self, descending: bool, models: &mut [T], model: impl Fn(&T) -> &Model) {
        let rank = |model: &Model| model.recommended.unwrap_or(u32::MAX);
        models.sort_by(|a, b| {
            let (a, b) = (model(a), model(b));
            let order = match self {
                // A lower rank is more recommended.
                Sort::Recommended => rank(b).cmp(&rank(a)),
                Sort::Accuracy => a.accuracy.cmp(&b.accuracy),
                Sort::Speed => a.speed.cmp(&b.speed),
                Sort::Size => a.file.size.cmp(&b.file.size),
            };
            if descending { order.reverse() } else { order }
        });
    }

    /// The select's icon for an order running from the most down, or up.
    fn icon(descending: bool) -> &'static str {
        if descending {
            "sort-descending"
        } else {
            "sort-ascending"
        }
    }
}

impl Choice for Sort {
    type Value = Sort;

    fn title(&self) -> SharedString {
        match self {
            Sort::Recommended => "Recommended",
            Sort::Accuracy => "Accuracy",
            Sort::Speed => "Speed",
            Sort::Size => "Size",
        }
        .into()
    }

    fn value(&self) -> &Sort {
        self
    }
}

/// What a download sends back to the main thread.
enum Update {
    Progress(u64, u64),
    Done(Result<Outcome, String>),
}

/// A row of the language filter: every language, `None`, or one a listed Model transcribes.
#[derive(Clone)]
struct LanguageFilter {
    code: Option<String>,
    name: SharedString,
}

impl Choice for LanguageFilter {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &Option<String> {
        &self.code
    }
}

impl LocalModels {
    /// Reads the models folder and makes the search field.
    pub(super) fn new(window: &mut Window, cx: &mut Context<NativeView>) -> Self {
        let library = Library::open()
            .inspect_err(|error| eprintln!("Could not find the models folder: {error:#}"))
            .ok();
        let downloaded = library
            .as_ref()
            .map(Library::downloaded)
            .unwrap_or_default();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search Models by name…"));
        let sort = cx.new(|cx| {
            Select::new(
                "Sort",
                vec![Sort::Recommended, Sort::Accuracy, Sort::Speed, Sort::Size],
                Some(0),
                cx,
            )
            .icon(Sort::icon(true))
        });
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |view, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    view.local_models.filtered(cx);
                }
            }),
            cx.subscribe(&sort, |view, select, Picked(sort): &Picked<Sort>, cx| {
                let models = &mut view.local_models;
                let (last, descending) = models.sorted;
                let descending = if *sort == last {
                    !descending
                } else {
                    sort.descending()
                };
                models.sorted = (*sort, descending);
                select.update(cx, |select, cx| select.set_icon(Sort::icon(descending), cx));
                models.filtered(cx);
            }),
        ];
        Self {
            library,
            downloaded,
            reread: false,
            downloads: HashMap::new(),
            failures: HashMap::new(),
            search,
            streaming: false,
            language: None,
            sort,
            sorted: (Sort::Recommended, true),
            rows: Rc::default(),
            // Every row is measured once, so the scrollbar spans the whole list from the start.
            list: ListState::new(0, ListAlignment::Top, px(200.0)).measure_all(),
            _subscriptions: subscriptions,
        }
    }

    /// The downloaded Models.
    pub(super) fn downloaded(&self) -> &[Downloaded] {
        &self.downloaded
    }

    /// Shows the list from its top once the search field, a filter or the order changed.
    fn filtered(&self, cx: &mut Context<NativeView>) {
        self.list.scroll_to(ListOffset::default());
        cx.notify();
    }
}

impl NativeView {
    /// The picked Local Model, while it is downloaded as last read.
    pub(super) fn picked_local_model(&self) -> Option<&Downloaded> {
        let slug = self.store.local_model()?;
        (self.local_models.downloaded.iter()).find(|downloaded| downloaded.model.slug == slug)
    }

    /// The models folder and the picked Local Model, read from disk now, as a job needs it.
    pub(super) fn local_model_on_disk(&self) -> Option<(&Library, Downloaded)> {
        let library = self.local_models.library.as_ref()?;
        Some((library, library.get(self.store.local_model()?)?))
    }

    /// The picked Local Model's name, from its download or the list, for the user.
    pub(super) fn local_model_name(&self) -> String {
        let slug = self.store.local_model().unwrap_or_default();
        self.picked_local_model()
            .map(|downloaded| downloaded.model.name.clone())
            .or_else(|| find(slug).map(|model| model.name.clone()))
            .unwrap_or_else(|| "The Local Model".to_owned())
    }

    /// Reads the models folder again when asked to, and updates the Transcription section's
    /// selects when the downloaded Models changed.
    pub(super) fn reread_local_models(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.local_models.reread) {
            return;
        }
        let Some(library) = self.local_models.library.as_ref() else {
            return;
        };
        let downloaded = library.downloaded();
        if downloaded != self.local_models.downloaded {
            self.local_models.downloaded = downloaded;
            self.refresh_transcription_selects(cx);
        }
    }

    /// Makes the Local Model `slug` the Transcription Model.
    fn use_local_model(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.forget_transcription_failures(self.transcription_provider());
        self.store.set_transcription_provider(Provider::Local.id());
        self.store.set_local_model(Some(slug));
        self.refresh_transcription_selects(cx);
        cx.notify();
    }

    /// Makes Apple's Model the Transcription Model.
    fn use_apple_model(&mut self, cx: &mut Context<Self>) {
        self.forget_transcription_failures(self.transcription_provider());
        self.store.set_transcription_provider(Provider::Apple.id());
        self.refresh_transcription_selects(cx);
        cx.notify();
    }

    /// Starts downloading `model`, unless it is already downloading.
    fn download_local_model(&mut self, model: &Model, cx: &mut Context<Self>) {
        let Some(library) = self.local_models.library.clone() else {
            return;
        };
        if self.local_models.downloads.contains_key(&model.slug) {
            return;
        }
        self.local_models.failures.remove(&model.slug);
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, updates) = async_channel::unbounded();
        let (stop, wanted) = (cancel.clone(), model.clone());
        let work = cx.background_executor().spawn(async move {
            let mut shown = u64::MAX;
            let progress = &mut |held: u64, total: u64| {
                // One update per thousandth, not per read.
                let thousandths = held * 1000 / total.max(1);
                if thousandths != shown {
                    shown = thousandths;
                    let _ = send.try_send(Update::Progress(held, total));
                }
            };
            let result = library.download(&wanted, progress, &stop);
            let _ = send.try_send(Update::Done(result.map_err(|error| error.to_string())));
        });
        let slug = model.slug.clone();
        let updates = cx.spawn(async move |view, cx| {
            let _work = work;
            while let Ok(update) = updates.recv().await {
                let done = view.update(cx, |view, cx| view.download_update(&slug, update, cx));
                if done.is_err() {
                    break;
                }
            }
        });
        self.local_models.downloads.insert(
            model.slug.clone(),
            Download {
                held: 0,
                total: model.file.size,
                cancel,
                _updates: updates,
            },
        );
        cx.notify();
    }

    fn download_update(&mut self, slug: &str, update: Update, cx: &mut Context<Self>) {
        let models = &mut self.local_models;
        match update {
            Update::Progress(held, total) => {
                if let Some(download) = models.downloads.get_mut(slug) {
                    (download.held, download.total) = (held, total);
                }
            }
            Update::Done(result) => {
                models.downloads.remove(slug);
                match result {
                    Ok(_) => models.reread = true,
                    Err(message) => {
                        models.failures.insert(slug.to_owned(), message);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Stops the download of `slug`, keeping what arrived for the next Download.
    fn cancel_download(&mut self, slug: &str, cx: &mut Context<Self>) {
        if let Some(download) = self.local_models.downloads.get(slug) {
            download.cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    /// Asks before deleting the downloaded Model `slug`; deleting the one in use picks Apple's.
    fn confirm_delete_local_model(
        &mut self,
        model: &Model,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_use = self.transcription_provider() == Provider::Local
            && self.store.local_model() == Some(model.slug.as_str());
        let description = if in_use {
            format!(
                "It frees {}. Transcripts will use Apple's Model until you pick another.",
                crumble_local_models::size_label(model.file.size)
            )
        } else {
            format!(
                "It frees {}. You can download it again.",
                crumble_local_models::size_label(model.file.size)
            )
        };
        let (view, slug, name) = (
            cx.entity().downgrade(),
            model.slug.clone(),
            model.name.clone(),
        );
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (view, slug) = (view.clone(), slug.clone());
            alert
                .entrance(DialogEntrance::Fade)
                .title(format!("Delete {name}?"))
                .description(description.clone())
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.delete_local_model(&slug, cx));
                    true
                })
        });
    }

    fn delete_local_model(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(library) = self.local_models.library.as_ref() else {
            return;
        };
        if let Err(error) = library.delete(slug) {
            eprintln!("Could not delete the Local Model: {error:#}");
            (self.local_models.failures).insert(slug.to_owned(), "Couldn't delete it.".to_owned());
        }
        if self.store.local_model() == Some(slug) {
            if self.transcription_provider() == Provider::Local {
                self.store.set_transcription_provider(Provider::Apple.id());
            }
            self.store.set_local_model(None);
        }
        self.local_models.downloaded = library.downloaded();
        self.refresh_transcription_selects(cx);
        cx.notify();
    }
}

/// "All languages", then every language a listed Model transcribes, by name.
fn language_filters() -> Vec<LanguageFilter> {
    let mut codes: Vec<&str> = catalog()
        .iter()
        .flat_map(|model| model.languages.iter().map(String::as_str))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    let mut languages: Vec<LanguageFilter> = codes
        .into_iter()
        .map(|code| LanguageFilter {
            name: crumble_transcribe::language_name(code).into(),
            code: Some(code.to_owned()),
        })
        .collect();
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    let mut items = vec![LanguageFilter {
        code: None,
        name: "All languages".into(),
    }];
    items.extend(languages);
    items
}

#[cfg(test)]
mod tests {
    use super::Sort;
    use crumble_local_models::{Model, catalog};

    #[test]
    fn a_sort_orders_models_either_way_and_keeps_ties_in_order() {
        let mut models: Vec<&Model> = catalog().iter().collect();
        Sort::Speed.apply(true, &mut models, |model| model);
        assert!(models.windows(2).all(|pair| pair[0].speed >= pair[1].speed));
        Sort::Accuracy.apply(true, &mut models, |model| model);
        assert!(models.windows(2).all(|pair| {
            pair[0].accuracy > pair[1].accuracy
                || (pair[0].accuracy == pair[1].accuracy && pair[0].speed >= pair[1].speed)
        }));
        Sort::Accuracy.apply(false, &mut models, |model| model);
        assert!(models.windows(2).all(|pair| {
            pair[0].accuracy < pair[1].accuracy
                || (pair[0].accuracy == pair[1].accuracy && pair[0].speed >= pair[1].speed)
        }));
        Sort::Size.apply(Sort::Size.descending(), &mut models, |model| model);
        assert!(
            models
                .windows(2)
                .all(|pair| pair[0].file.size <= pair[1].file.size)
        );
        Sort::Recommended.apply(true, &mut models, |model| model);
        assert!(models[0].recommended.is_some());
        Sort::Recommended.apply(false, &mut models, |model| model);
        assert!(
            models
                .last()
                .is_some_and(|model| model.recommended.is_some())
        );
    }
}
