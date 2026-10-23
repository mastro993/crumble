//! The rows of the Model selects under each Provider select: a remote Provider's Models, listed
//! or typed, or for Transcripts the downloaded Local Models.

use super::super::select::Choice;
use crumble_local_models::Downloaded;
use crumble_remote::{Job, Listed, Remote};
use gpui_kit::SharedString;

/// A row of a remote Provider's Model select: the Model's id, listed or typed.
#[derive(Clone)]
pub(super) struct ModelItem(pub(super) String);

impl Choice for ModelItem {
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
}

/// A row of the Local Model select: a downloaded Local Model by name, its slug its value.
#[derive(Clone)]
pub(super) struct LocalModelItem {
    slug: String,
    name: SharedString,
}

impl Choice for LocalModelItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &String {
        &self.slug
    }
}

/// The Models `remote`'s select for `job` lists, and the one it shows picked: Crumble's
/// suggestions, then those `listed`, then `saved` when neither has it, so a pick is never
/// silently replaced; `saved` is picked, else the default.
pub(super) fn model_choices(
    remote: &Remote,
    job: Job,
    saved: Option<&str>,
    listed: Option<&Listed>,
) -> (Vec<ModelItem>, Option<usize>) {
    let mut models = remote.models(job, listed);
    let selected = remote.model(job, saved).map(|picked| {
        models
            .iter()
            .position(|model| model == picked)
            .unwrap_or_else(|| {
                models.push(picked.to_owned());
                models.len() - 1
            })
    });
    (models.into_iter().map(ModelItem).collect(), selected)
}

/// The Local Model select's rows, each `downloaded` Local Model, and the row of `picked`, the
/// Local Model's slug; a picked one that isn't downloaded has no row.
pub(super) fn local_model_choices(
    downloaded: &[Downloaded],
    picked: Option<&str>,
) -> (Vec<LocalModelItem>, Option<usize>) {
    let items: Vec<LocalModelItem> = (downloaded.iter())
        .map(|downloaded| LocalModelItem {
            slug: downloaded.model.slug.clone(),
            name: downloaded.model.name.clone().into(),
        })
        .collect();
    let selected = picked.and_then(|picked| items.iter().position(|item| item.slug == picked));
    (items, selected)
}

#[cfg(test)]
mod tests {
    use super::{LocalModelItem, ModelItem, local_model_choices, model_choices};
    use crate::native_view::select::Choice;
    use crumble_local_models::Downloaded;
    use crumble_remote::{Job, Listed, Provider, Remote};

    fn remote(provider: Provider) -> &'static Remote {
        provider
            .remote()
            .unwrap_or_else(|| panic!("{provider:?} isn't remote"))
    }

    fn ids(items: &[ModelItem]) -> Vec<&str> {
        items.iter().map(|item| item.0.as_str()).collect()
    }

    #[test]
    fn a_select_lists_suggestions_then_listed_models_and_picks_the_default() {
        let anthropic = remote(Provider::Anthropic);
        let listed = Listed {
            chat: vec!["claude-opus-5-5".to_owned(), "claude-3-haiku".to_owned()],
            speech: Vec::new(),
        };
        let (models, selected) = model_choices(anthropic, Job::Summary, None, Some(&listed));
        assert_eq!(
            ids(&models),
            [
                "claude-sonnet-5-5",
                "claude-opus-5-5",
                "claude-haiku-4-5",
                "claude-3-haiku"
            ]
        );
        assert_eq!(selected, Some(0));
        let (_, selected) = model_choices(
            anthropic,
            Job::Summary,
            Some("claude-3-haiku"),
            Some(&listed),
        );
        assert_eq!(selected, Some(3));
    }

    #[test]
    fn a_saved_model_no_list_has_stays_as_an_extra_row() {
        let server = remote(Provider::OpenAiCompatible);
        // Nothing to pick until the server lists its Models.
        let (models, selected) = model_choices(server, Job::Summary, None, None);
        assert!(models.is_empty());
        assert_eq!(selected, None);
        let (models, selected) = model_choices(server, Job::Summary, Some("llama3.2"), None);
        assert_eq!((ids(&models), selected), (vec!["llama3.2"], Some(0)));
    }

    #[test]
    fn local_model_rows_are_the_downloaded_ones_and_a_pick_not_downloaded_has_none() {
        let [first, second, ..] = crumble_local_models::catalog() else {
            panic!("the Local Models list has fewer than two Models");
        };
        let downloaded = [first, second].map(|model| Downloaded {
            model: model.clone(),
            offered: true,
        });
        let slugs = |items: &[LocalModelItem]| -> Vec<String> {
            items.iter().map(|item| item.value().clone()).collect()
        };
        let (items, selected) = local_model_choices(&downloaded, Some(&second.slug));
        assert_eq!(slugs(&items), [first.slug.clone(), second.slug.clone()]);
        assert_eq!(items[1].title().as_ref(), second.name);
        assert_eq!(selected, Some(1));
        let (items, selected) = local_model_choices(&downloaded, Some("deleted-in-finder"));
        assert_eq!((items.len(), selected), (2, None));
        assert_eq!(local_model_choices(&downloaded, None).1, None);
    }
}
