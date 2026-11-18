//! Small JPEG copies of pages, for covers and the reader's page strip, cached
//! on disk under `app_cache_dir/manga/thumbs/<volume id>/`.
//!
//! A volume's folder goes when a scan finds its files changed or gone
//! (`purge`), so a thumbnail never outlives the page it shows. A series' own
//! cover (`cover.*`, or a remote one) is cached as `cover-<series>-<version>/`:
//! a new version drops the older ones (`drop_other_covers`) and a series that
//! is gone loses its own (`sweep_covers`).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{ImageReader, Limits};

/// Width of a page in the reader's strip and of a volume in the series list.
pub const THUMB_WIDTH: u32 = 240;
/// Width of a cover in the library grid and the "continue reading" shelf.
pub const COVER_WIDTH: u32 = 480;
const QUALITY: u8 = 82;

/// Decoding limits: no real page is this large, and a crafted header could
/// otherwise ask for gigabytes.
pub(crate) fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    limits
}

/// `bytes` scaled down to at most `max_width` wide (never up), as JPEG.
pub fn make(bytes: &[u8], max_width: u32) -> Result<Vec<u8>, image::ImageError> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits());
    let image = reader.decode()?;
    let image = if image.width() > max_width {
        let height = ((image.height() as u64 * max_width as u64) / image.width().max(1) as u64).max(1) as u32;
        image.thumbnail_exact(max_width, height)
    } else {
        image
    };
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, QUALITY).encode_image(&image.to_rgb8())?;
    Ok(out)
}

pub fn root(cache_dir: &Path) -> PathBuf {
    cache_dir.join("manga").join("thumbs")
}

/// Where one thumbnail is cached. Ids and numbers only, checked by the caller.
pub fn path(cache_dir: &Path, volume_id: &str, page: usize, width: u32) -> PathBuf {
    root(cache_dir).join(volume_id).join(format!("{page}-{width}.jpg"))
}

/// The cached thumbnail, or a new one made with `page_bytes` and saved. A page
/// the decoder cannot read (an AVIF, a broken file) is served as it is.
pub fn cached(
    cache_dir: &Path,
    volume_id: &str,
    page: usize,
    width: u32,
    page_bytes: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let target = path(cache_dir, volume_id, page, width);
    if let Ok(bytes) = std::fs::read(&target) {
        return Some(bytes);
    }
    let original = page_bytes()?;
    match make(&original, width) {
        Ok(bytes) => {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Written aside and renamed, so a reader never sees half a file.
            let partial = target.with_extension("part");
            if std::fs::write(&partial, &bytes).is_ok() && std::fs::rename(&partial, &target).is_err() {
                let _ = std::fs::remove_file(&partial);
            }
            Some(bytes)
        }
        Err(error) => {
            log::debug!("manga thumbnail for {volume_id}/{page} failed: {error}");
            Some(original)
        }
    }
}

/// Drops the thumbnails of these volumes.
pub fn purge(cache_dir: &Path, volume_ids: &[String]) {
    for id in volume_ids {
        if super::protocol::is_opaque_id(id) {
            let _ = std::fs::remove_dir_all(root(cache_dir).join(id));
        }
    }
}

const COVER_PREFIX: &str = "cover-";
/// Length of a series id (a hyphenated UUID).
const SERIES_ID_LEN: usize = 36;

/// The series a cached cover folder belongs to, or `None` for a volume's.
fn cover_series(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(COVER_PREFIX)?;
    let series = rest.get(..SERIES_ID_LEN)?;
    (rest.as_bytes().get(SERIES_ID_LEN) == Some(&b'-')).then_some(series)
}

/// The cover folders cached for a series other than `keep`, once a new
/// version of its cover has been cached as `keep`.
pub fn drop_other_covers(cache_dir: &Path, keep: &str) {
    let Some(series) = cover_series(keep) else { return };
    let Ok(entries) = std::fs::read_dir(root(cache_dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name != keep && cover_series(name) == Some(series) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Drops the cached covers of series that are no longer in the library
/// (`live`): a folder removed, a server forgotten, a series gone from it.
pub fn sweep_covers(cache_dir: &Path, live: &std::collections::HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(root(cache_dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if cover_series(name).is_some_and(|series| !live.contains(series)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// `sweep_covers` against the library as it is now.
pub fn sweep_library_covers(db: &crate::db::Db, cache_dir: &Path) {
    match db.manga_series_ids() {
        Ok(live) => sweep_covers(cache_dir, &live),
        Err(error) => log::debug!("manga cover sweep skipped: {error}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn thumbnails_shrink_keep_the_aspect_and_never_grow() {
        let small = make(&png(1000, 1500), 240).unwrap();
        let decoded = image::load_from_memory(&small).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (240, 360));
        assert_eq!(&small[..3], &[0xFF, 0xD8, 0xFF]);

        let tiny = image::load_from_memory(&make(&png(100, 150), 240).unwrap()).unwrap();
        assert_eq!(tiny.width(), 100);
        assert!(make(b"not an image", 240).is_err());
    }

    #[test]
    fn a_thumbnail_is_made_once_and_purged_with_its_volume() {
        let cache = std::env::temp_dir().join(format!("ntdc-thumbs-{}", uuid::Uuid::new_v4()));
        let id = "0b5c7d0e-0000-4000-8000-000000000001".to_string();
        let first = cached(&cache, &id, 0, 240, || Some(png(600, 900))).unwrap();
        let again = cached(&cache, &id, 0, 240, || panic!("decoded twice")).unwrap();
        assert_eq!(first, again);
        purge(&cache, std::slice::from_ref(&id));
        assert!(!path(&cache, &id, 0, 240).exists());
        // An unreadable page is served as it came.
        assert_eq!(cached(&cache, &id, 1, 240, || Some(b"raw".to_vec())).unwrap(), b"raw");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn covers_of_gone_series_and_old_versions_are_dropped() {
        let cache = std::env::temp_dir().join(format!("ntdc-covers-{}", uuid::Uuid::new_v4()));
        let kept = "11111111-0000-4000-8000-000000000001";
        let gone = "22222222-0000-4000-8000-000000000002";
        let volume = "33333333-0000-4000-8000-000000000003";
        for bucket in [format!("cover-{kept}-old"), format!("cover-{kept}-new"), format!("cover-{gone}-v1"), volume.to_string()] {
            cached(&cache, &bucket, 0, 240, || Some(png(60, 90))).unwrap();
        }
        let exists = |bucket: &str| root(&cache).join(bucket).exists();

        drop_other_covers(&cache, &format!("cover-{kept}-new"));
        assert!(!exists(&format!("cover-{kept}-old")));
        assert!(exists(&format!("cover-{kept}-new")));
        assert!(exists(&format!("cover-{gone}-v1")), "another series' cover stays");

        sweep_covers(&cache, &[kept.to_string()].into_iter().collect());
        assert!(!exists(&format!("cover-{gone}-v1")));
        assert!(exists(&format!("cover-{kept}-new")));
        assert!(exists(volume), "a volume's thumbnails are not covers");
        let _ = std::fs::remove_dir_all(cache);
    }
}
