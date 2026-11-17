//! `manga://`: how the webview gets pages, thumbnails and covers without being
//! able to read anything else on disk.
//!
//! URLs carry opaque ids only:
//!
//! ```text
//! manga://localhost/page/<volume id>/<n>    a page, as stored
//! manga://localhost/thumb/<volume id>/<n>   a small JPEG of it (cached)
//! manga://localhost/cover/<series id>       the series' cover (cached)
//! ```
//!
//! (On Windows the webview spells it `http://manga.localhost/…`; the page
//! builds URLs with `convertFileSrc`, which may also encode the slashes.)
//!
//! Every id is looked up in the database, and the file it names comes from
//! there, never from the URL. Anything that is not exactly one of these shapes
//! is a 404, whatever it contains.

use std::io::Read;
use std::path::Path;

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use super::archive::{self, safe_entry_name, MAX_ENTRY_BYTES};
use super::store::{VolumeLocation, FORMAT_ARCHIVE, FORMAT_FOLDER};
use super::{is_image_name, thumbs};
use crate::db::Db;
use crate::state::AppState;

pub const SCHEME: &str = "manga";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asset {
    Page { volume: String, page: usize },
    Thumb { volume: String, page: usize },
    Cover { series: String },
}

/// What our own ids look like: a UUID, or anything as plain. Never a path.
pub fn is_opaque_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn page_number(raw: &str) -> Option<usize> {
    (!raw.is_empty() && raw.len() <= 6 && raw.bytes().all(|b| b.is_ascii_digit())).then(|| raw.parse().ok()).flatten()
}

/// The asset a request path asks for, or `None` for anything else. Only `%2F`
/// is decoded (what `convertFileSrc` makes of a slash); any other escape, or
/// any character outside ids and digits, is refused rather than interpreted.
pub fn parse(path: &str) -> Option<Asset> {
    let raw = path.strip_prefix('/').unwrap_or(path);
    let mut decoded = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => match (chars.next(), chars.next()) {
                (Some('2'), Some('F' | 'f')) => decoded.push('/'),
                _ => return None,
            },
            c if c.is_ascii_alphanumeric() || c == '-' || c == '/' => decoded.push(c),
            _ => return None,
        }
    }
    let parts: Vec<&str> = decoded.split('/').collect();
    match parts.as_slice() {
        ["page", id, n] if is_opaque_id(id) => Some(Asset::Page { volume: id.to_string(), page: page_number(n)? }),
        ["thumb", id, n] if is_opaque_id(id) => Some(Asset::Thumb { volume: id.to_string(), page: page_number(n)? }),
        ["cover", id] if is_opaque_id(id) => Some(Asset::Cover { series: id.to_string() }),
        _ => None,
    }
}

/// An image file, if it is one and is not huge.
fn read_image_file(path: &Path) -> Option<Vec<u8>> {
    if !path.to_str().is_some_and(is_image_name) {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_ENTRY_BYTES).then_some(bytes)
}

/// A page of a folder volume: `name` must be a plain relative path, and once
/// links are resolved it must still be inside the folder.
fn read_folder_page(dir: &Path, name: &str) -> Option<Vec<u8>> {
    if safe_entry_name(name).as_deref() != Some(name) {
        return None;
    }
    let dir = dir.canonicalize().ok()?;
    let file = dir.join(name).canonicalize().ok()?;
    if !file.starts_with(&dir) {
        return None;
    }
    read_image_file(&file)
}

/// The bytes of page `page` (0-based) of a volume.
pub fn page_bytes(location: &VolumeLocation, page: usize) -> Option<Vec<u8>> {
    let name = location.pages.get(page)?;
    match location.format.as_str() {
        FORMAT_FOLDER => read_folder_page(Path::new(&location.source_ref), name),
        FORMAT_ARCHIVE => archive::read_page_at(Path::new(&location.source_ref), name)
            .map_err(|error| log::debug!("manga page {}/{page}: {error}", location.id))
            .ok(),
        _ => None,
    }
}

/// What the protocol answers, before it becomes an HTTP response.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub content_type: Option<&'static str>,
    pub etag: Option<String>,
    pub body: Vec<u8>,
}

impl Reply {
    fn status(status: u16) -> Self {
        Reply { status, content_type: None, etag: None, body: Vec::new() }
    }
}

/// The type of an image by its first bytes: the file name is not evidence.
pub fn sniff(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'B', b'M', ..] => "image/bmp",
        [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f', ..] => "image/avif",
        _ => "application/octet-stream",
    }
}

fn file_signature(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(format!("{}-{modified}", meta.len()))
}

/// Answers one request. `cache_dir` is where thumbnails are kept.
pub fn serve(db: &Db, cache_dir: &Path, method: &str, path: &str, if_none_match: Option<&str>) -> Reply {
    if method != "GET" && method != "HEAD" {
        return Reply::status(405);
    }
    let Some(asset) = parse(path) else { return Reply::status(404) };

    let (etag, body) = match &asset {
        Asset::Page { volume, page } | Asset::Thumb { volume, page } => {
            let Ok(Some(location)) = db.manga_volume_location(volume) else { return Reply::status(404) };
            if *page >= location.pages.len() {
                return Reply::status(404);
            }
            let thumb = matches!(asset, Asset::Thumb { .. });
            let etag = format!("\"{}-{}{page}\"", location.signature.as_deref().unwrap_or("0"), if thumb { 't' } else { 'p' });
            if if_none_match == Some(etag.as_str()) {
                return Reply { etag: Some(etag), ..Reply::status(304) };
            }
            let body = if thumb {
                thumbs::cached(cache_dir, volume, *page, thumbs::THUMB_WIDTH, || page_bytes(&location, *page))
            } else {
                page_bytes(&location, *page)
            };
            (etag, body)
        }
        Asset::Cover { series } => {
            let Ok(Some((cover, first))) = db.manga_series_cover(series) else { return Reply::status(404) };
            // The folder's own cover image, else the first page of the first volume.
            let own = cover.as_deref().map(Path::new).and_then(|path| Some((path, file_signature(path)?)));
            if let Some((path, signature)) = own {
                let etag = format!("\"{signature}-c\"");
                if if_none_match == Some(etag.as_str()) {
                    return Reply { etag: Some(etag), ..Reply::status(304) };
                }
                let key = format!("cover-{series}");
                let body = cached_cover(cache_dir, &key, &signature, || read_image_file(path));
                (etag, body)
            } else {
                let Some(first) = first else { return Reply::status(404) };
                let etag = format!("\"{}-c\"", first.signature.as_deref().unwrap_or("0"));
                if if_none_match == Some(etag.as_str()) {
                    return Reply { etag: Some(etag), ..Reply::status(304) };
                }
                let body = thumbs::cached(cache_dir, &first.id, 0, thumbs::COVER_WIDTH, || page_bytes(&first, 0));
                (etag, body)
            }
        }
    };
    match body {
        Some(body) => Reply {
            status: 200,
            content_type: Some(sniff(&body)),
            etag: Some(etag),
            body: if method == "HEAD" { Vec::new() } else { body },
        },
        None => Reply::status(404),
    }
}

/// A series' own cover file, thumbnailed and cached under a key that changes
/// with the file.
fn cached_cover(cache_dir: &Path, key: &str, signature: &str, bytes: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let bucket = format!("{key}-{}", signature.replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    thumbs::cached(cache_dir, &bucket, 0, thumbs::COVER_WIDTH, bytes)
}

fn response(reply: Reply) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header("X-Content-Type-Options", "nosniff");
    if let Some(content_type) = reply.content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(etag) = reply.etag {
        // Short-lived: a rescan can change what an id shows, and revalidating
        // is one database lookup.
        builder = builder.header(header::ETAG, etag).header(header::CACHE_CONTROL, "private, max-age=600");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "no-store");
    }
    builder.body(reply.body).unwrap_or_else(|_| Response::new(Vec::new()))
}

/// The protocol handler: the work runs on a blocking thread, never on the
/// webview's.
pub fn handle<R: Runtime>(app: &AppHandle<R>, request: Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app.try_state::<AppState>() else {
            responder.respond(response(Reply::status(503)));
            return;
        };
        let cache_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("nt-companion"));
        let if_none_match = request.headers().get(header::IF_NONE_MATCH).and_then(|value| value.to_str().ok());
        let reply = serve(&state.db, &cache_dir, request.method().as_str(), request.uri().path(), if_none_match);
        responder.respond(response(reply));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::store::tests::{series, temp_db, volume};
    use crate::manga::store::{VolumeRecord, SOURCE_LOCAL};
    use crate::manga::thumbs::tests::png;

    const VOL: &str = "0b5c7d0e-0000-4000-8000-0000000000aa";

    #[test]
    fn only_the_three_shapes_parse() {
        assert_eq!(parse(&format!("/page/{VOL}/3")), Some(Asset::Page { volume: VOL.into(), page: 3 }));
        assert_eq!(parse(&format!("/thumb%2F{VOL}%2f0")), Some(Asset::Thumb { volume: VOL.into(), page: 0 }));
        assert_eq!(parse("/cover%2Fabc-1"), Some(Asset::Cover { series: "abc-1".into() }));
        for bad in [
            "/page/../../etc/passwd/0",
            "/page/..%2F..%2Fetc%2Fpasswd/0",
            "/page/C:%5Cwindows/0",
            "/page/%2E%2E/0",
            "/page/abc/-1",
            "/page/abc/1.5",
            "/page/abc/9999999",
            "/page/abc",
            "/page/abc/1/2",
            "/file/abc/1",
            "/cover/",
            "/cover/a b",
            "/C:/Users/x/manga/001.jpg",
            "/%2Fetc%2Fpasswd",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn unknown_ids_and_paths_are_404_and_only_get_is_served() {
        let (db, path) = temp_db();
        let cache = std::env::temp_dir();
        assert_eq!(serve(&db, &cache, "GET", &format!("/page/{VOL}/0"), None).status, 404);
        assert_eq!(serve(&db, &cache, "GET", "/etc/passwd", None).status, 404);
        assert_eq!(serve(&db, &cache, "GET", "/cover/nope", None).status, 404);
        assert_eq!(serve(&db, &cache, "POST", &format!("/page/{VOL}/0"), None).status, 405);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pages_thumbnails_and_covers_are_served_from_a_scanned_folder() {
        let (db, db_path) = temp_db();
        let root = std::env::temp_dir().join(format!("ntdc-proto-{}", uuid::Uuid::new_v4()));
        let folder = root.join("Vol 1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("001.png"), png(600, 900)).unwrap();
        std::fs::write(root.join("secret.png"), png(10, 10)).unwrap();
        let cache = root.join("cache");

        let record = VolumeRecord {
            source_ref: folder.to_string_lossy().into_owned(),
            format: FORMAT_FOLDER,
            // A tampered row: the second "page" tries to climb out.
            pages: vec!["001.png".into(), "../secret.png".into()],
            ..volume(VOL, 1, 0)
        };
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", &root.to_string_lossy()).unwrap();
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s1", "Series", vec![record])]).unwrap();

        let page = serve(&db, &cache, "GET", &format!("/page/{VOL}/0"), None);
        assert_eq!((page.status, page.content_type), (200, Some("image/png")));
        let etag = page.etag.clone().unwrap();
        assert_eq!(serve(&db, &cache, "GET", &format!("/page/{VOL}/0"), Some(&etag)).status, 304);
        assert_eq!(serve(&db, &cache, "HEAD", &format!("/page/{VOL}/0"), None).body.len(), 0);
        assert_eq!(serve(&db, &cache, "GET", &format!("/page/{VOL}/1"), None).status, 404);
        assert_eq!(serve(&db, &cache, "GET", &format!("/page/{VOL}/2"), None).status, 404);

        let thumb = serve(&db, &cache, "GET", &format!("/thumb/{VOL}/0"), None);
        assert_eq!(thumb.content_type, Some("image/jpeg"));
        assert_eq!(image::load_from_memory(&thumb.body).unwrap().width(), thumbs::THUMB_WIDTH);

        let cover = serve(&db, &cache, "GET", "/cover/s1", None);
        assert_eq!(cover.content_type, Some("image/jpeg"));
        assert_eq!(image::load_from_memory(&cover.body).unwrap().width(), thumbs::COVER_WIDTH);

        drop(db);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn content_type_comes_from_the_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(sniff(b"<html>"), "application/octet-stream");
    }
}
