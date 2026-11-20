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
/// Width of a cover in the library grid, the "continue reading" shelf and a
/// series' page. Drawn at 136–200 CSS px (176 on a series' page), so 320 is
/// sharp up to a 180 % display scale; each one decoded is less than half the
/// memory a 480 one took, and the grid holds 120 at a time.
pub const COVER_WIDTH: u32 = 320;
/// Cover widths used before, whose files `sweep_library_covers` deletes once.
const OLD_COVER_WIDTHS: &[u32] = &[480];
/// Which cover width the cache was last cleaned for (`kv`).
const COVER_WIDTH_KEY: &str = "manga:covers:width";
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

/// Where the strip thumbnail of a page of a volume on a server is cached. Not
/// `path`: there, page 0 of a remote volume is its cover image.
pub fn strip_path(cache_dir: &Path, volume_id: &str, page: usize) -> PathBuf {
    root(cache_dir).join(volume_id).join(format!("strip-{page}-{THUMB_WIDTH}.jpg"))
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
    cached_at(&path(cache_dir, volume_id, page, width), width, &format!("{volume_id}/{page}"), page_bytes)
}

/// [`cached`], at any place.
pub fn cached_at(target: &Path, width: u32, label: &str, page_bytes: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    if let Ok(bytes) = std::fs::read(target) {
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
            if std::fs::write(&partial, &bytes).is_ok() && std::fs::rename(&partial, target).is_err() {
                let _ = std::fs::remove_file(&partial);
            }
            Some(bytes)
        }
        Err(error) => {
            log::debug!("manga thumbnail for {label} failed: {error}");
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
    // Once per cover width: the covers of an older width are only disk.
    let current = COVER_WIDTH.to_string();
    if db.get_kv(COVER_WIDTH_KEY).as_deref() != Some(current.as_str()) {
        drop_cover_widths(cache_dir, OLD_COVER_WIDTHS);
        let _ = db.set_kv(COVER_WIDTH_KEY, &current);
    }
}

/// Deletes cached covers (`<page>-<width>.jpg`, in any folder of the cache)
/// of these widths. Thumbnails (`THUMB_WIDTH`) and the strip's own files are
/// another name and stay.
pub fn drop_cover_widths(cache_dir: &Path, widths: &[u32]) {
    let Ok(folders) = std::fs::read_dir(root(cache_dir)) else { return };
    for folder in folders.flatten() {
        let Ok(files) = std::fs::read_dir(folder.path()) else { continue };
        for file in files.flatten() {
            let name = file.file_name();
            let Some(name) = name.to_str() else { continue };
            let old = name
                .strip_suffix(".jpg")
                .and_then(|stem| stem.split_once('-'))
                .is_some_and(|(page, width)| page.parse::<usize>().is_ok() && width.parse::<u32>().is_ok_and(|w| widths.contains(&w)));
            if old {
                let _ = std::fs::remove_file(file.path());
            }
        }
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

    #[test]
    fn covers_of_an_old_width_go_and_thumbnails_stay() {
        let cache = std::env::temp_dir().join(format!("ntdc-widths-{}", uuid::Uuid::new_v4()));
        let volume = "44444444-0000-4000-8000-000000000004";
        let bucket = "cover-55555555-0000-4000-8000-000000000005-v1";
        for (folder, width) in [(volume, 480), (volume, THUMB_WIDTH), (volume, COVER_WIDTH), (bucket, 480)] {
            cached(&cache, folder, 0, width, || Some(png(600, 900))).unwrap();
        }
        let strip = strip_path(&cache, volume, 0);
        std::fs::write(&strip, b"strip").unwrap();

        drop_cover_widths(&cache, &[480]);
        assert!(!path(&cache, volume, 0, 480).exists());
        assert!(!path(&cache, bucket, 0, 480).exists());
        assert!(path(&cache, volume, 0, THUMB_WIDTH).exists(), "thumbnails stay");
        assert!(path(&cache, volume, 0, COVER_WIDTH).exists(), "covers of the current width stay");
        assert!(strip.exists(), "the strip's files are another name");
        let _ = std::fs::remove_dir_all(cache);
    }
}
