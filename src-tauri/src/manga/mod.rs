//! The manga reader: local volumes (image folders, CBZ/ZIP) with their mokuro
//! text, served to the webview through the `manga://` protocol.
//!
//! Nothing here is shown by path: the UI only ever sees opaque ids, and Rust
//! resolves them against the database (see `protocol`).

pub mod archive;
pub mod crop;
pub mod library;
pub mod mokuro;
pub mod ocr;
pub mod protocol;
pub mod reader;
pub mod session;
pub mod store;
pub mod suwayomi;
pub mod thumbs;
pub mod webdav;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// What the manga reader keeps in memory, in `AppState`.
#[derive(Default)]
pub struct Runtime {
    /// A library scan is running: a second one would only redo its work.
    pub scanning: AtomicBool,
    pub last_scan: Mutex<Option<Instant>>,
    /// The volume the reader window shows, or is about to.
    pub reader: Mutex<Option<reader::ReaderTarget>>,
    /// What is being read and counted (see `session`). Never held across an
    /// `.await`.
    pub tracking: Mutex<session::Tracking>,
    /// The reader window has the focus (from its window events).
    pub reader_focused: AtomicBool,
    /// WebDAV sources being synced now, and when each last was.
    pub webdav_syncing: Mutex<HashSet<String>>,
    pub webdav_last_sync: Mutex<HashMap<String, Instant>>,
    /// Where remote pages, texts and downloads come from. Set once at start.
    pub remote: OnceLock<Arc<webdav::remote::Remote>>,
    /// The latest progress save of each remote volume, so only the last of a
    /// burst is written to the server (see `webdav::progress`).
    pub progress_push: Mutex<HashMap<String, u64>>,
    /// The built-in OCR (see `ocr`). Set once at start.
    pub ocr: OnceLock<Arc<ocr::Service>>,
}

impl Runtime {
    pub fn remote(&self) -> Option<Arc<webdav::remote::Remote>> {
        self.remote.get().cloned()
    }

    pub fn ocr(&self) -> Option<Arc<ocr::Service>> {
        self.ocr.get().cloned()
    }
}

/// Extensions the webview can show as a page.
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "avif", "bmp"];

pub fn is_image_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// Natural order, the way a person numbers pages: `001 < 2 < 10`, case
/// ignored, folder by folder. Ties fall back to the plain comparison so the
/// order is total and stable.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.split('/');
    let mut right = b.split('/');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match natural_cmp_part(x, y) {
                Ordering::Equal => continue,
                other => return other,
            },
        }
    }
}

fn natural_cmp_part(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let start_a = i;
            while i < a.len() && a[i].is_ascii_digit() { i += 1; }
            let start_b = j;
            while j < b.len() && b[j].is_ascii_digit() { j += 1; }
            let x = trim_zeros(&a[start_a..i]);
            let y = trim_zeros(&b[start_b..j]);
            match x.len().cmp(&y.len()).then_with(|| x.cmp(y)) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        match a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase()) {
            Ordering::Equal => { i += 1; j += 1; }
            other => return other,
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let first = digits.iter().position(|&d| d != b'0').unwrap_or(digits.len());
    &digits[first..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_reads_numbers_as_numbers() {
        let mut names = vec!["10.jpg", "2.jpg", "001.jpg", "Page 3.png", "page 20.png", "1b.jpg", "1a.jpg"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["001.jpg", "1a.jpg", "1b.jpg", "2.jpg", "10.jpg", "Page 3.png", "page 20.png"]);
    }

    #[test]
    fn natural_order_compares_folder_by_folder() {
        let mut names = vec!["v1/ch10/1.png", "v1/ch2/10.png", "v1/ch2-extra/1.png", "v1/ch2/9.png"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["v1/ch2/9.png", "v1/ch2/10.png", "v1/ch2-extra/1.png", "v1/ch10/1.png"]);
    }

    #[test]
    fn only_images_are_pages() {
        assert!(is_image_name("001.JPG"));
        assert!(is_image_name("a/b.webp"));
        assert!(!is_image_name("vol.mokuro"));
        assert!(!is_image_name("jpg"));
    }
}
