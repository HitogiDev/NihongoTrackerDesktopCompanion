//! Series from extensions in the library: a series is a row of
//! `manga_series` (`source = 'suwayomi'`, `source_ref` = Suwayomi's id of
//! it), each chapter a volume (`format = 'suwayomi-chapter'`, `source_ref` =
//! the chapter's id, `pages_json` = its pages' server paths once fetched).
//! The reader, the session, logs, mining and the OCR then work on chapters
//! like on any volume; `webdav::remote::Remote` serves their pages (offline
//! copy, cache, then the server).
//!
//! A series from a source marked NSFW or MIXED is added with `is_adult`, and
//! keeps it whatever it is linked to (`manga_series.source_adult`), so
//! Discord never shows it (`discord::decide`).

use super::graphql::{Chapter, Content, Manga, Server, SwError};
use super::{catalog, Service, SOURCE_KIND};
use crate::db::Db;
use crate::manga::store::{RemoteVolumeRecord, FORMAT_SUWAYOMI_CHAPTER};

pub fn series_id(source_row: &str, manga_id: i64) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("suwayomi\0{source_row}\0manga\0{manga_id}").as_bytes()).to_string()
}

pub fn volume_id(source_row: &str, chapter_id: i64) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("suwayomi\0{source_row}\0chapter\0{chapter_id}").as_bytes()).to_string()
}

/// A chapter as a library volume. Its signature changes when the source
/// re-uploads it, so stale pages are fetched again.
pub fn chapter_record(source_row: &str, chapter: &Chapter) -> RemoteVolumeRecord {
    RemoteVolumeRecord {
        id: volume_id(source_row, chapter.id),
        title: chapter.name.trim().to_string(),
        source_ref: chapter.id.to_string(),
        format: FORMAT_SUWAYOMI_CHAPTER,
        position: chapter.source_order,
        pages: Vec::new(),
        signature: Some(format!("{}-{}", chapter.url, chapter.upload_date.as_deref().unwrap_or(""))),
        mokuro_path: None,
        layers: Vec::new(),
        cover: None,
        size: None,
    }
}

/// Whether a series from this source is adult.
pub fn adult(manga: &Manga) -> bool {
    // Without its source's rating, assume the worst.
    manga.source.as_ref().map_or(Content::Nsfw, |source| source.content_warning).adult()
}

/// Puts a series and its chapters in the library (or refreshes them).
/// Returns our id of it.
pub fn store(db: &Db, source_row: &str, manga: &Manga, chapters: &[Chapter]) -> Result<String, SwError> {
    let disk = |error: rusqlite::Error| {
        log::warn!("Suwayomi: could not store a series: {error}");
        SwError::Disk
    };
    let id = series_id(source_row, manga.id);
    let cover = manga.thumbnail_url.as_deref().and_then(super::graphql::server_path);
    db.upsert_extension_series(&id, source_row, &manga.id.to_string(), manga.title.trim(), cover.as_deref(), adult(manga)).map_err(disk)?;
    let records: Vec<RemoteVolumeRecord> = chapters.iter().map(|chapter| chapter_record(source_row, chapter)).collect();
    db.apply_remote_volumes(&id, None, cover.as_deref(), &records).map_err(disk)?;
    Ok(id)
}

/// Adds a series from a server (asking it for the chapters).
pub async fn add(db: &Db, server: &Server, source_row: &str, manga_id: i64, refresh: bool) -> Result<String, SwError> {
    let (manga, chapters) = server.manga(manga_id, refresh).await?;
    let id = store(db, source_row, &manga, &chapters)?;
    // Kept in the server's own library too, so it is not cleaned up.
    if let Err(error) = server.set_in_library(manga_id, true).await {
        log::info!("Suwayomi: series {manga_id} not marked in its library: {error}");
    }
    Ok(id)
}

/// The series' page of a source, with our id if it is in the library.
pub fn detail(service: &Service, db: &Db, source_row: &str, manga: Manga, chapters: &[Chapter]) -> catalog::ExtMangaDetail {
    let id = series_id(source_row, manga.id);
    let in_library = db.manga_series(&id).ok().flatten().is_some_and(|series| series.source == SOURCE_KIND);
    let mut icon = |path: &str| service.image_token(source_row, path);
    let mut chapters: Vec<catalog::ExtChapter> = chapters.iter().map(catalog::chapter).collect();
    chapters.sort_by_key(|chapter| chapter.position);
    catalog::ExtMangaDetail {
        adult: adult(&manga),
        thumb: manga.thumbnail_url.as_deref().and_then(&mut icon),
        source: manga.source.clone().map(|source| catalog::source(source, &mut icon)),
        id: manga.id,
        title: manga.title,
        author: manga.author.filter(|s| !s.trim().is_empty()),
        artist: manga.artist.filter(|s| !s.trim().is_empty()),
        description: manga.description.filter(|s| !s.trim().is_empty()),
        genres: manga.genre,
        status: manga.status,
        series_id: in_library.then_some(id),
        chapters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::temp_db;
    use crate::manga::suwayomi::graphql::Source;

    fn manga(content: Content) -> Manga {
        Manga {
            id: 2,
            title: "終活ルームメイト".into(),
            thumbnail_url: Some("/api/v1/manga/2/thumbnail".into()),
            author: None,
            artist: None,
            description: None,
            genre: Vec::new(),
            status: None,
            source_id: Some("5".into()),
            source: Some(Source {
                id: "5".into(),
                name: "S".into(),
                lang: "ja".into(),
                display_name: "S (JA)".into(),
                content_warning: content,
                supports_latest: true,
                icon_url: String::new(),
                extension: None,
            }),
        }
    }

    fn chapters() -> Vec<Chapter> {
        (1..=3)
            .map(|n| Chapter {
                id: 175 + n,
                name: format!("第{n}話"),
                chapter_number: n as f64,
                scanlator: None,
                upload_date: Some("1792638000000".into()),
                source_order: n,
                page_count: -1,
                url: format!("u{n}"),
            })
            .collect()
    }

    #[test]
    fn a_series_from_an_nsfw_source_is_adult_and_stays_adult() {
        let (db, path) = temp_db();
        db.add_manga_source("suwayomi-managed", SOURCE_KIND, "Suwayomi", "managed").unwrap();
        let id = store(&db, "suwayomi-managed", &manga(Content::Nsfw), &chapters()).unwrap();
        let series = db.manga_series(&id).unwrap().unwrap();
        assert!(series.is_adult);
        assert_eq!(series.source, "suwayomi");
        // Linked to a media that is not adult: still adult for Discord.
        db.link_manga_series(&id, Some("123"), Some("T"), false).unwrap();
        assert!(db.manga_series(&id).unwrap().unwrap().is_adult);
        db.link_manga_series(&id, None, None, false).unwrap();
        assert!(db.manga_series(&id).unwrap().unwrap().is_adult);

        // Chapters are volumes, in the source's order, without pages yet.
        let volumes = db.manga_volumes(&id).unwrap();
        assert_eq!(volumes.iter().map(|v| v.title.as_str()).collect::<Vec<_>>(), ["第1話", "第2話", "第3話"]);
        assert!(volumes.iter().all(|v| v.format == FORMAT_SUWAYOMI_CHAPTER && v.page_count == 0));
        assert_eq!(volumes[0].id, volume_id("suwayomi-managed", 176));

        // A SAFE one is not; a MIXED one is (it may be).
        let mut safe = manga(Content::Safe);
        safe.id = 3;
        assert!(!db.manga_series(&store(&db, "suwayomi-managed", &safe, &[]).unwrap()).unwrap().unwrap().is_adult);
        let mut mixed = manga(Content::Mixed);
        mixed.id = 4;
        assert!(db.manga_series(&store(&db, "suwayomi-managed", &mixed, &[]).unwrap()).unwrap().unwrap().is_adult);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn refreshing_keeps_pages_and_progress_of_unchanged_chapters() {
        let (db, path) = temp_db();
        db.add_manga_source("suwayomi-managed", SOURCE_KIND, "Suwayomi", "managed").unwrap();
        let id = store(&db, "suwayomi-managed", &manga(Content::Safe), &chapters()).unwrap();
        let first = volume_id("suwayomi-managed", 176);
        db.set_manga_volume_pages(&first, &["/api/v1/manga/2/chapter/1/page/0".to_string()]).unwrap();
        db.set_manga_progress(&first, 0, true).unwrap();
        let mut more = chapters();
        more.push(Chapter { id: 179, name: "第4話".into(), chapter_number: 4.0, scanlator: None, upload_date: None, source_order: 4, page_count: -1, url: "u4".into() });
        store(&db, "suwayomi-managed", &manga(Content::Safe), &more).unwrap();
        let volumes = db.manga_volumes(&id).unwrap();
        assert_eq!(volumes.len(), 4);
        assert_eq!(volumes[0].page_count, 1);
        assert!(volumes[0].finished);
        // Its cover is the server's thumbnail path, which only Rust reads.
        assert_eq!(db.manga_series_origin(&id).unwrap().unwrap().2.as_deref(), Some("/api/v1/manga/2/thumbnail"));
        let _ = std::fs::remove_file(path);
    }
}
