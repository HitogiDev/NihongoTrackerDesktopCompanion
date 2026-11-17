//! The `.mokuro` format (kha-white/mokuro; only its file format is read here):
//! a volume's pages, each with the text blocks the OCR found on it.
//!
//! ```text
//! { version, title, title_uuid, volume, volume_uuid,
//!   pages: [{ img_path, img_width, img_height,
//!             blocks: [{ box: [x1, y1, x2, y2], vertical, font_size,
//!                        lines: [..], lines_coords: [[[x, y] ×4] ..] }] }] }
//! ```
//!
//! Other tools add their own fields (mokuro-bunko writes `ocr_engine`, newer
//! mokuro versions add per-block extras): anything unknown is ignored and
//! anything missing takes a default, so a file is only rejected when it is not
//! JSON at all. The same page shape is what the OCR cache stores
//! (`manga_page_text.blocks_json`) and what the reader draws.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A `.mokuro` past this size is not one (a real volume is well under 5 MB).
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MokuroVolume {
    pub version: Option<String>,
    pub title: Option<String>,
    pub title_uuid: Option<String>,
    pub volume: Option<String>,
    pub volume_uuid: Option<String>,
    pub pages: Vec<MokuroPage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MokuroPage {
    /// Where the image was when mokuro read it, relative to the volume.
    pub img_path: String,
    pub img_width: f64,
    pub img_height: f64,
    pub blocks: Vec<MokuroBlock>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MokuroBlock {
    /// `[x1, y1, x2, y2]` in image pixels.
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
    pub vertical: bool,
    /// In image pixels, like the box.
    pub font_size: f64,
    pub lines: Vec<String>,
    /// Four corners per line, in image pixels.
    pub lines_coords: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, thiserror::Error)]
pub enum MokuroError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the .mokuro file is too large")]
    TooLarge,
    #[error("not a .mokuro file: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn parse(bytes: &[u8]) -> Result<MokuroVolume, MokuroError> {
    Ok(serde_json::from_slice(bytes)?)
}

pub fn load(path: &Path) -> Result<MokuroVolume, MokuroError> {
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Err(MokuroError::TooLarge);
    }
    parse(&std::fs::read(path)?)
}

fn normalized(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_lowercase()
}

fn stem(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

/// The mokuro page for each of the volume's pages, in the volume's order.
///
/// mokuro names each image by where it was when it ran (`vol1/001.jpg`), and
/// the volume may have moved or been packed since, so pages are matched by the
/// end of their path, then by file name without extension. When nothing
/// matches at all but the counts agree, the order is trusted.
pub fn match_pages(volume: &MokuroVolume, page_names: &[String]) -> Vec<Option<MokuroPage>> {
    let known: Vec<String> = volume.pages.iter().map(|page| normalized(&page.img_path)).collect();
    let mut taken = vec![false; known.len()];
    let mut matched: Vec<Option<MokuroPage>> = vec![None; page_names.len()];

    let mut pass = |same: &dyn Fn(&str, &str) -> bool| {
        for (slot, name) in matched.iter_mut().zip(page_names) {
            if slot.is_some() {
                continue;
            }
            let name = normalized(name);
            if let Some(index) = (0..known.len()).find(|&index| !taken[index] && same(&known[index], &name)) {
                taken[index] = true;
                *slot = Some(volume.pages[index].clone());
            }
        }
    };
    pass(&|known, name| known == name || known.ends_with(&format!("/{name}")) || name.ends_with(&format!("/{known}")));
    pass(&|known, name| stem(known) == stem(name));

    if matched.iter().all(Option::is_none) && volume.pages.len() == page_names.len() {
        return volume.pages.iter().cloned().map(Some).collect();
    }
    matched
}

/// The text of each page of a volume, for the reader: its `.mokuro` when it
/// has one, matched to its pages, and the stored OCR text for the pages that
/// file does not cover. `None` for a page with no text at all.
pub fn volume_text(db: &crate::db::Db, volume_id: &str) -> Result<Vec<Option<MokuroPage>>, String> {
    let Some(location) = db.manga_volume_location(volume_id).map_err(|e| e.to_string())? else {
        return Err("Unknown volume".into());
    };
    let mut pages = match location.mokuro_path.as_deref() {
        Some(path) => match load(Path::new(path)) {
            Ok(volume) => match_pages(&volume, &location.pages),
            Err(error) => {
                log::warn!("unreadable .mokuro for {volume_id}: {error}");
                vec![None; location.pages.len()]
            }
        },
        None => vec![None; location.pages.len()],
    };
    for (page, json, _engine) in db.manga_page_text(volume_id).map_err(|e| e.to_string())? {
        if let Some(slot) = usize::try_from(page).ok().and_then(|page| pages.get_mut(page)) {
            if slot.is_none() {
                *slot = serde_json::from_str::<MokuroPage>(&json).ok();
            }
        }
    }
    Ok(pages)
}

/// The text of a page, as the reader reads it: each block's lines joined.
#[cfg_attr(not(test), allow(dead_code))]
pub fn page_text(page: &MokuroPage) -> Vec<String> {
    page.blocks.iter().map(|block| block.lines.concat()).filter(|text| !text.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written by hand: two pages, one block each, plus fields this reader does
    /// not know about at every level.
    const MINIMAL: &str = r#"{
        "version": "0.2.1",
        "title": "テスト",
        "title_uuid": "00000000-0000-0000-0000-000000000001",
        "volume": "第1巻",
        "volume_uuid": "00000000-0000-0000-0000-000000000002",
        "ocr_engine": "some-engine",
        "pages": [
            {
                "version": "0.2.1",
                "img_path": "第1巻/002.jpg",
                "img_width": 800,
                "img_height": 1200,
                "blocks": [
                    {
                        "box": [500, 100, 600, 400],
                        "vertical": true,
                        "font_size": 32.5,
                        "lines": ["こんにちは", "世界"],
                        "lines_coords": [[[560, 100], [600, 100], [600, 300], [560, 300]]],
                        "prob": 0.98
                    }
                ]
            },
            { "img_path": "第1巻/001.jpg", "img_width": 800, "img_height": 1200, "blocks": [] }
        ]
    }"#;

    #[test]
    fn reads_a_minimal_file_and_ignores_unknown_fields() {
        let volume = parse(MINIMAL.as_bytes()).unwrap();
        assert_eq!(volume.title.as_deref(), Some("テスト"));
        assert_eq!(volume.pages.len(), 2);
        let block = &volume.pages[0].blocks[0];
        assert_eq!(block.bbox, [500.0, 100.0, 600.0, 400.0]);
        assert!(block.vertical);
        assert_eq!(block.font_size, 32.5);
        assert_eq!(page_text(&volume.pages[0]), ["こんにちは世界"]);
    }

    #[test]
    fn missing_fields_take_defaults_and_garbage_is_rejected() {
        let volume = parse(r#"{"pages":[{"img_path":"1.png","blocks":[{"lines":["あ"]}]}]}"#.as_bytes()).unwrap();
        assert_eq!(volume.pages[0].blocks[0].bbox, [0.0; 4]);
        assert!(!volume.pages[0].blocks[0].vertical);
        assert!(parse(b"{}").unwrap().pages.is_empty());
        assert!(parse(b"not json").is_err());
    }

    #[test]
    fn the_block_box_keeps_its_name_for_the_reader() {
        let json = serde_json::to_value(MokuroBlock { bbox: [1.0, 2.0, 3.0, 4.0], ..Default::default() }).unwrap();
        assert_eq!(json["box"], serde_json::json!([1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn pages_match_by_path_then_name_then_order() {
        let volume = parse(MINIMAL.as_bytes()).unwrap();
        // Packed in a CBZ without the folder, and in a different order.
        let names = vec!["001.jpg".to_string(), "002.jpg".to_string(), "003.jpg".to_string()];
        let matched = match_pages(&volume, &names);
        assert_eq!(matched[0].as_ref().unwrap().img_path, "第1巻/001.jpg");
        assert_eq!(matched[1].as_ref().unwrap().blocks.len(), 1);
        assert!(matched[2].is_none());

        // Converted to another format: same stem, different extension.
        let names = vec!["第1巻/001.webp".to_string(), "第1巻/002.webp".to_string()];
        assert_eq!(match_pages(&volume, &names)[1].as_ref().unwrap().img_path, "第1巻/002.jpg");

        // Renamed entirely, same count: the order is all there is.
        let names = vec!["a.png".to_string(), "b.png".to_string()];
        assert_eq!(match_pages(&volume, &names)[0].as_ref().unwrap().img_path, "第1巻/002.jpg");
    }

    #[test]
    fn a_volume_takes_its_mokuro_text_and_the_ocr_cache_fills_the_gaps() {
        use crate::manga::store::tests::{series, temp_db, volume};
        use crate::manga::store::{VolumeRecord, SOURCE_LOCAL};
        let (db, path) = temp_db();
        let mokuro = std::env::temp_dir().join(format!("ntdc-{}.mokuro", uuid::Uuid::new_v4()));
        std::fs::write(&mokuro, MINIMAL).unwrap();
        let record = VolumeRecord {
            pages: vec!["001.jpg".into(), "002.jpg".into(), "003.jpg".into()],
            mokuro_path: Some(mokuro.to_string_lossy().into_owned()),
            ..volume("v1", 1, 0)
        };
        db.add_manga_source("src", SOURCE_LOCAL, "M", "/m").unwrap();
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s", "S", vec![record])]).unwrap();
        let ocr = MokuroPage { img_width: 10.0, img_height: 10.0, blocks: vec![MokuroBlock { lines: vec!["三".into()], ..Default::default() }], ..Default::default() };
        db.set_manga_page_text("v1", 2, &serde_json::to_string(&ocr).unwrap(), "builtin").unwrap();
        // The OCR cache never replaces mokuro's text.
        db.set_manga_page_text("v1", 1, &serde_json::to_string(&ocr).unwrap(), "builtin").unwrap();

        let text = volume_text(&db, "v1").unwrap();
        assert!(text[0].as_ref().unwrap().blocks.is_empty());
        assert_eq!(page_text(text[1].as_ref().unwrap()), ["こんにちは世界"]);
        assert_eq!(page_text(text[2].as_ref().unwrap()), ["三"]);
        assert!(volume_text(&db, "nope").is_err());
        drop(db);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(mokuro);
    }

    #[test]
    fn chapters_with_the_same_file_names_keep_their_own_text() {
        let volume = parse(r#"{"pages":[
            {"img_path":"v/ch1/001.jpg","blocks":[{"lines":["一"]}]},
            {"img_path":"v/ch2/001.jpg","blocks":[{"lines":["二"]}]}
        ]}"#.as_bytes()).unwrap();
        let names = vec!["ch1/001.jpg".to_string(), "ch2/001.jpg".to_string()];
        let matched = match_pages(&volume, &names);
        assert_eq!(page_text(matched[0].as_ref().unwrap()), ["一"]);
        assert_eq!(page_text(matched[1].as_ref().unwrap()), ["二"]);
    }
}
