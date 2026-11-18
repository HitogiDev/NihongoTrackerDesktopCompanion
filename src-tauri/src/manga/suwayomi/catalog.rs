//! What the store and the catalogues look like to the UI, and which of it
//! the UI may see.
//!
//! Nothing here carries a URL of the server: images are opaque tokens
//! (`manga://localhost/ext/<token>`) that Rust resolves to a server path
//! and fetches itself (see `Service::image_token`). Adult content
//! (`NSFW`, and `MIXED`, which may be) is left out unless the user turned on
//! "show adult sources": the filtering is here, not in the page.

use serde::Serialize;

use super::graphql::{self, Content};

/// An extension of the store, as a card shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtExtension {
    pub pkg_name: String,
    pub name: String,
    pub lang: String,
    pub version: String,
    /// `safe`, `mixed` or `nsfw`.
    pub content: String,
    pub installed: bool,
    pub has_update: bool,
    pub obsolete: bool,
    /// Token for `manga://…/ext/<token>`.
    pub icon: Option<String>,
}

/// A source of an installed extension.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtSource {
    /// Suwayomi's id: a 64-bit number, as a string.
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub lang: String,
    pub content: String,
    pub supports_latest: bool,
    pub icon: Option<String>,
    pub pkg_name: Option<String>,
}

/// A series in a catalogue.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtManga {
    /// Suwayomi's id of the series.
    pub id: i64,
    pub title: String,
    pub thumb: Option<String>,
    /// Ours, once it is in the library.
    pub series_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtBrowse {
    pub mangas: Vec<ExtManga>,
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtChapter {
    pub id: i64,
    pub name: String,
    /// `-1` when the source gives none.
    pub number: f64,
    pub scanlator: Option<String>,
    /// Milliseconds since the epoch.
    pub uploaded_at: Option<i64>,
    /// Reading order (the source's).
    pub position: i64,
    /// `-1` until its pages were fetched once.
    pub page_count: i64,
}

/// A series' page from a source: what it is and its chapters.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ExtMangaDetail {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub status: Option<String>,
    pub thumb: Option<String>,
    pub source: Option<ExtSource>,
    /// From a source marked NSFW or MIXED: added to the library as adult.
    pub adult: bool,
    pub series_id: Option<String>,
    pub chapters: Vec<ExtChapter>,
}

/// Whether something with this rating may be shown.
pub fn visible(content: Content, show_adult: bool) -> bool {
    show_adult || !content.adult()
}

/// The store's extensions the user may see, installed and updatable first,
/// then by name. The local source is not an extension of any store.
pub fn extensions(list: Vec<graphql::Extension>, show_adult: bool, icon: &mut dyn FnMut(&str) -> Option<String>) -> Vec<ExtExtension> {
    let mut out: Vec<ExtExtension> = list
        .into_iter()
        .filter(|extension| visible(extension.content_warning, show_adult))
        .filter(|extension| extension.pkg_name != "eu.kanade.tachiyomi.source.local")
        .map(|extension| ExtExtension {
            icon: icon(&extension.icon_url),
            pkg_name: extension.pkg_name,
            name: extension.name,
            lang: extension.lang,
            version: extension.version_name,
            content: extension.content_warning.as_str().to_string(),
            installed: extension.is_installed,
            has_update: extension.has_update,
            obsolete: extension.is_obsolete,
        })
        .collect();
    out.sort_by_key(|extension| (!extension.has_update, !extension.installed, extension.name.to_lowercase()));
    out
}

pub fn source(source: graphql::Source, icon: &mut dyn FnMut(&str) -> Option<String>) -> ExtSource {
    ExtSource {
        icon: icon(&source.icon_url),
        display_name: if source.display_name.is_empty() { source.name.clone() } else { source.display_name },
        id: source.id,
        name: source.name,
        lang: source.lang,
        content: source.content_warning.as_str().to_string(),
        supports_latest: source.supports_latest,
        pkg_name: source.extension.map(|extension| extension.pkg_name),
    }
}

/// The sources the user may browse: not the server's local folder (id `0`),
/// adult ones only when shown.
pub fn sources(list: Vec<graphql::Source>, show_adult: bool, icon: &mut dyn FnMut(&str) -> Option<String>) -> Vec<ExtSource> {
    let mut out: Vec<ExtSource> = list
        .into_iter()
        .filter(|source| source.id != "0" && visible(source.content_warning, show_adult))
        .map(|item| source(item, icon))
        .collect();
    out.sort_by_key(|source| (source.lang != "ja", source.display_name.to_lowercase()));
    out
}

pub fn chapter(chapter: &graphql::Chapter) -> ExtChapter {
    ExtChapter {
        id: chapter.id,
        name: chapter.name.clone(),
        number: chapter.chapter_number,
        scanlator: chapter.scanlator.clone().filter(|s| !s.trim().is_empty()),
        uploaded_at: chapter.upload_date.as_deref().and_then(|raw| raw.parse().ok()).filter(|ms: &i64| *ms > 0),
        position: chapter.source_order,
        page_count: chapter.page_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(pkg: &str, content: Content, installed: bool, update: bool) -> graphql::Extension {
        graphql::Extension {
            pkg_name: pkg.into(),
            name: pkg.to_uppercase(),
            lang: "ja".into(),
            version_name: "1.0".into(),
            content_warning: content,
            is_installed: installed,
            has_update: update,
            is_obsolete: false,
            icon_url: format!("/api/v1/extension/icon/{pkg}"),
        }
    }

    #[test]
    fn adult_extensions_are_hidden_by_default() {
        let list = vec![
            extension("a", Content::Safe, false, false),
            extension("b", Content::Mixed, false, false),
            extension("c", Content::Nsfw, true, false),
            extension("d", Content::Safe, true, true),
        ];
        let mut icon = |path: &str| Some(format!("t-{}", path.len()));
        let shown = extensions(list.clone(), false, &mut icon);
        assert_eq!(shown.iter().map(|e| e.pkg_name.as_str()).collect::<Vec<_>>(), ["d", "a"]);
        assert!(shown.iter().all(|e| e.content == "safe"));
        // Only with the explicit setting, and still updates first.
        let all = extensions(list, true, &mut icon);
        assert_eq!(all.iter().map(|e| e.pkg_name.as_str()).collect::<Vec<_>>(), ["d", "c", "a", "b"]);
        // The default of the setting is off.
        assert!(!crate::config::Settings::default().manga_ext_show_adult);
    }

    #[test]
    fn sources_leave_out_the_local_folder_and_adult_ones() {
        let make = |id: &str, lang: &str, content: Content| graphql::Source {
            id: id.into(),
            name: id.into(),
            lang: lang.into(),
            display_name: String::new(),
            content_warning: content,
            supports_latest: true,
            icon_url: String::new(),
            extension: None,
        };
        let list = vec![make("0", "localsourcelang", Content::Safe), make("9", "en", Content::Safe), make("7", "ja", Content::Safe), make("8", "ja", Content::Nsfw)];
        let mut icon = |_: &str| None;
        let shown = sources(list, false, &mut icon);
        // Japanese first.
        assert_eq!(shown.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["7", "9"]);
        assert_eq!(shown[0].display_name, "7");
    }

    #[test]
    fn a_chapter_keeps_its_order_and_reads_its_date() {
        let item = graphql::Chapter {
            id: 176,
            name: "第１話".into(),
            chapter_number: -1.0,
            scanlator: Some(" ".into()),
            upload_date: Some("1541689200000".into()),
            source_order: 1,
            page_count: -1,
            url: "x".into(),
        };
        let out = chapter(&item);
        assert_eq!(out.uploaded_at, Some(1_541_689_200_000));
        assert_eq!(out.scanlator, None);
        assert_eq!(out.position, 1);
    }
}
