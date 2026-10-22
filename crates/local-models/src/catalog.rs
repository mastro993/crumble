//! The Local Models Crumble offers: Handy's model list (`handy/catalog.json`, MIT, see
//! `handy/LICENSE`), copied unchanged and synced by `.github/workflows/handy-catalog.yml`
//! (ADR 0010). Each entry is read down to what Crumble shows and downloads: its default quant.

use std::sync::LazyLock;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// The copied list, as Handy publishes it.
const CATALOG: &str = include_str!("../handy/catalog.json");

/// The list format this parser reads; a sync bringing another fails the catalog test.
const CATALOG_VERSION: u32 = 2;

/// One Model a user can download.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// Its stable id, such as `whisper-medium`.
    pub slug: String,
    /// Its name for the user, such as "Whisper Medium".
    pub name: String,
    /// One line about it.
    pub description: String,
    /// The ISO 639 codes of the languages it transcribes.
    pub languages: Vec<String>,
    /// Whether it can make a Transcript while the audio plays in, for live Transcripts later.
    pub streaming: bool,
    /// Whether it can tell which language is spoken.
    pub detects_language: bool,
    /// Whether it gives no times of its own, so each chunk of audio becomes one segment.
    pub rough_timestamps: bool,
    /// How few words it gets wrong, 0 to 100, from Handy's measurements.
    pub accuracy: u8,
    /// How fast it runs, 0 to 100, on Handy's reference laptop.
    pub speed: u8,
    /// Its place among Handy's recommended Models, `None` when not recommended.
    pub recommended: Option<u32>,
    /// The file Crumble downloads.
    pub file: ModelFile,
}

/// One file on Hugging Face, pinned to a revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFile {
    /// The repository, such as `handy-computer/whisper-medium-gguf`.
    pub repo: String,
    /// The commit the file is read at.
    pub revision: String,
    /// Its name in the repository and in the models folder.
    pub filename: String,
    /// Its length in bytes.
    pub size: u64,
    /// Its sha256 in lowercase hex.
    pub sha256: String,
}

impl ModelFile {
    /// Where Hugging Face serves it.
    pub(crate) fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.filename
        )
    }
}

/// What the Local Models page narrows the list to.
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter<'a> {
    /// Words the name must contain, ignoring case; blank matches every Model.
    pub query: &'a str,
    /// Whether only Models that can stream are shown.
    pub streaming: bool,
    /// An ISO 639 code the Model must transcribe, or `None` for every language.
    pub language: Option<&'a str>,
}

impl Model {
    /// Whether it is shown under `filter`.
    pub fn matches(&self, filter: &Filter) -> bool {
        let name = self.name.to_lowercase();
        let query = filter.query.trim().to_lowercase();
        query.split_whitespace().all(|word| name.contains(word))
            && (!filter.streaming || self.streaming)
            && filter
                .language
                .is_none_or(|language| self.languages.iter().any(|code| code == language))
    }
}

/// Every Model in the copied list, in Handy's order. Empty, after logging why, if the list does
/// not read, which the catalog test rules out before a release.
pub fn catalog() -> &'static [Model] {
    static MODELS: LazyLock<Vec<Model>> = LazyLock::new(|| {
        parse(CATALOG).unwrap_or_else(|error| {
            eprintln!("Could not read the Local Models list: {error:#}");
            Vec::new()
        })
    });
    &MODELS
}

/// The Model `slug` in the copied list.
pub fn find(slug: &str) -> Option<&'static Model> {
    catalog().iter().find(|model| model.slug == slug)
}

#[derive(Deserialize)]
struct Catalog {
    catalog_version: u32,
    models: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    revision: String,
    slug: String,
    name: String,
    description: String,
    languages: Vec<String>,
    capabilities: Capabilities,
    speed_score: u8,
    accuracy_score: u8,
    files: Vec<Quant>,
    default_quant: String,
    recommended: bool,
    recommended_rank: Option<u32>,
}

#[derive(Deserialize)]
struct Capabilities {
    streaming: bool,
    lang_detect: bool,
    timestamps: String,
}

#[derive(Deserialize)]
struct Quant {
    filename: String,
    quant: String,
    size_bytes: u64,
    sha256: Option<String>,
}

/// The Models in `json`, a list in Handy's format; an entry without its default quant or a
/// sha256 for it is left out, as Crumble could not check its download.
fn parse(json: &str) -> anyhow::Result<Vec<Model>> {
    let catalog: Catalog = serde_json::from_str(json).context("not Handy's list format")?;
    if catalog.catalog_version != CATALOG_VERSION {
        bail!(
            "list format {} is not {CATALOG_VERSION}",
            catalog.catalog_version
        );
    }
    Ok(catalog.models.into_iter().filter_map(model).collect())
}

fn model(entry: Entry) -> Option<Model> {
    let quant = entry
        .files
        .into_iter()
        .find(|file| file.quant == entry.default_quant)?;
    Some(Model {
        slug: entry.slug,
        name: entry.name,
        description: entry.description,
        languages: entry.languages,
        streaming: entry.capabilities.streaming,
        detects_language: entry.capabilities.lang_detect,
        rough_timestamps: entry.capabilities.timestamps == "none",
        accuracy: entry.accuracy_score,
        speed: entry.speed_score,
        recommended: entry
            .recommended
            .then(|| entry.recommended_rank.unwrap_or(u32::MAX)),
        file: ModelFile {
            repo: entry.id,
            revision: entry.revision,
            filename: quant.filename,
            size: quant.size_bytes,
            sha256: quant.sha256?.to_lowercase(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{CATALOG, Filter, catalog, find, parse};

    #[test]
    fn the_copied_list_reads_whole() {
        let raw: serde_json::Value = serde_json::from_str(CATALOG).unwrap_or_default();
        let entries = raw["models"].as_array().map_or(0, Vec::len);
        assert!(entries > 0);
        assert_eq!(catalog().len(), entries, "every entry has a checkable file");
        let whisper = find("whisper-medium");
        assert!(whisper.is_some_and(|model| {
            model.languages.len() > 90
                && !model.rough_timestamps
                && model.file.filename.ends_with(".gguf")
                && model.file.sha256.len() == 64
                && model
                    .file
                    .url()
                    .starts_with("https://huggingface.co/handy-computer/")
        }));
    }

    #[test]
    fn another_list_format_is_refused() {
        let error = parse(r#"{"catalog_version": 3, "models": []}"#);
        assert!(error.is_err());
        assert!(parse(r#"{"catalog_version": 2, "models": []}"#).is_ok_and(|m| m.is_empty()));
    }

    #[test]
    fn an_entry_without_its_default_quant_is_left_out() {
        let json = r#"{"catalog_version": 2, "models": [{
            "id": "o/r", "revision": "abc", "slug": "s", "name": "N", "description": "",
            "license": "mit", "languages": ["en"],
            "capabilities": {"streaming": false, "lang_detect": false, "timestamps": "none"},
            "speed_score": 1, "accuracy_score": 2, "default_quant": "Q8_0",
            "files": [{"filename": "f.gguf", "quant": "F16", "size_bytes": 1, "sha256": "AA"}],
            "recommended": false, "recommended_rank": null}]}"#;
        assert!(parse(json).is_ok_and(|models| models.is_empty()));
        let json = json.replace("F16", "Q8_0");
        let models = parse(&json).unwrap_or_default();
        assert_eq!(models.len(), 1);
        assert!(models[0].rough_timestamps);
        assert_eq!(models[0].file.sha256, "aa");
        assert_eq!(models[0].recommended, None);
    }

    #[test]
    fn filters_narrow_by_name_streaming_and_language() {
        let all = catalog();
        let shown = |filter: Filter| all.iter().filter(|model| model.matches(&filter)).count();
        assert_eq!(shown(Filter::default()), all.len());
        assert!(
            shown(Filter {
                query: "  WHISPER med ",
                ..Filter::default()
            }) >= 1
        );
        assert_eq!(
            shown(Filter {
                query: "no such model",
                ..Filter::default()
            }),
            0
        );
        let streaming = shown(Filter {
            streaming: true,
            ..Filter::default()
        });
        assert!(streaming > 0 && streaming < all.len());
        let italian = Filter {
            language: Some("it"),
            ..Filter::default()
        };
        assert!(
            all.iter()
                .filter(|m| m.matches(&italian))
                .all(|m| { m.languages.iter().any(|code| code == "it") })
        );
    }
}
