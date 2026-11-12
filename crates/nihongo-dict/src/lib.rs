//! Yomitan dictionary lookup, over the vendored hoshidicts C++ engine.
//!
//! The crate is deliberately free of Tauri and of anything app-shaped: the
//! desktop companion links it, and the same crate is meant to back an HTTP
//! service for the web texthooker.
//!
//! Layers, bottom to top:
//! - [`ffi`] — the raw `cxx` bridge. Nested C++ structs are flattened by the
//!   shim in `cpp/shim.cc`, because cxx cannot see through
//!   `std::vector<SomeStruct>`.
//! - [`Dictionary`] — a safe wrapper that owns the engine, puts an LRU in front
//!   of lookups and can warm the memory map at start-up.

use std::num::NonZeroUsize;
use std::sync::Mutex;

use lru::LruCache;

#[cxx::bridge(namespace = "ntdict")]
pub mod ffi {
    /// Result of importing one Yomitan `.zip`.
    #[derive(Debug, Clone)]
    pub struct ImportSummary {
        pub success: bool,
        pub title: String,
        pub term_count: u64,
        pub meta_count: u64,
        pub errors: Vec<String>,
    }

    /// One definition, tagged with the dictionary it came from. `text` is
    /// Yomitan structured content — a JSON tree, not plain prose.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Gloss {
        pub dict_name: String,
        pub text: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FreqHit {
        pub dict_name: String,
        pub display: String,
        pub value: i32,
    }

    /// One lookup hit: the surface form that matched, the lemma it deinflects
    /// to, and how it got there.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LookupRow {
        /// The substring of the input that matched.
        pub matched: String,
        /// The deinflected form that was looked up.
        pub deinflected: String,
        /// The deinflection chain, in order.
        pub process: Vec<String>,
        pub expression: String,
        pub reading: String,
        pub rules: String,
        pub glossaries: Vec<Gloss>,
        pub frequencies: Vec<FreqHit>,
    }

    unsafe extern "C++" {
        include!("shim.h");

        type Engine;

        fn new_engine() -> UniquePtr<Engine>;
        fn import_dictionary(zip_path: &str, out_dir: &str) -> ImportSummary;

        fn add_term_dict(self: Pin<&mut Engine>, path: &str);
        fn add_freq_dict(self: Pin<&mut Engine>, path: &str);
        fn lookup(&self, text: &str, max_results: i32) -> Vec<LookupRow>;
    }
}

pub use ffi::{FreqHit, Gloss, ImportSummary, LookupRow};

/// Import a Yomitan `.zip` into `out_dir/<dictionary title>`.
pub fn import(zip_path: &str, out_dir: &str) -> ImportSummary {
    ffi::import_dictionary(zip_path, out_dir)
}

/// How many distinct lookup keys to remember. Japanese prose repeats vocabulary
/// and particles heavily, so even a small cache pays for itself.
const DEFAULT_CACHE: usize = 4096;

/// `Lookup::lookup`'s default `scan_length`: the engine never looks further than
/// this many characters past the offset it was given.
pub const SCAN_LENGTH: usize = 16;

/// A set of dictionaries plus the deconjugator, with an LRU in front.
///
/// The C++ engine is not thread-safe, so it sits behind a mutex. That is fine
/// for a popup and for a modest service; a busy HTTP server would want one
/// `Dictionary` per worker, sharing nothing — the underlying data is a read-only
/// memory map, so a second engine costs little beyond its own cache.
pub struct Dictionary {
    engine: Mutex<cxx::UniquePtr<ffi::Engine>>,
    cache: Mutex<LruCache<(String, i32), Vec<LookupRow>>>,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
}

// Everything that touches the engine goes through the mutex above.
unsafe impl Send for Dictionary {}
unsafe impl Sync for Dictionary {}

impl Dictionary {
    pub fn new() -> Self {
        Self::with_cache(DEFAULT_CACHE)
    }

    pub fn with_cache(entries: usize) -> Self {
        let size = NonZeroUsize::new(entries.max(1)).expect("non-zero");
        Self {
            engine: Mutex::new(ffi::new_engine()),
            cache: Mutex::new(LruCache::new(size)),
            hits: Default::default(),
            misses: Default::default(),
        }
    }

    pub fn add_term_dict(&self, path: &str) {
        self.engine.lock().unwrap().pin_mut().add_term_dict(path);
        self.clear_cache();
    }

    pub fn add_freq_dict(&self, path: &str) {
        self.engine.lock().unwrap().pin_mut().add_freq_dict(path);
        self.clear_cache();
    }

    /// The engine scans substrings from `SCAN_LENGTH` characters down to 1, so
    /// nothing beyond that window can affect the answer. Truncating the cache
    /// key there is exact, not approximate, and it makes two different lines
    /// that happen to share an opening share a cache entry.
    fn cache_key(text: &str, max_results: i32) -> (String, i32) {
        let head: String = text.chars().take(SCAN_LENGTH).collect();
        (head, max_results)
    }

    /// Look `text` up, consulting the cache first.
    pub fn lookup(&self, text: &str, max_results: i32) -> Vec<LookupRow> {
        let key = Self::cache_key(text, max_results);
        if let Some(hit) = self.cache.lock().unwrap().get(&key) {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return hit.clone();
        }
        self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let rows = self.engine.lock().unwrap().lookup(text, max_results);
        self.cache.lock().unwrap().put(key, rows.clone());
        rows
    }

    /// Look up without touching the cache. For measuring, mostly.
    pub fn lookup_uncached(&self, text: &str, max_results: i32) -> Vec<LookupRow> {
        self.engine.lock().unwrap().lookup(text, max_results)
    }

    /// Resolve every character offset of a line, the way a popup has to when the
    /// reader can hover anywhere in it. One entry per offset, empty where
    /// nothing matched.
    pub fn resolve_line(&self, line: &str, max_results: i32) -> Vec<Vec<LookupRow>> {
        line.char_indices()
            .map(|(at, _)| self.lookup(&line[at..], max_results))
            .collect()
    }

    /// Same, bypassing the cache.
    pub fn resolve_line_uncached(&self, line: &str, max_results: i32) -> Vec<Vec<LookupRow>> {
        line.char_indices()
            .map(|(at, _)| self.lookup_uncached(&line[at..], max_results))
            .collect()
    }

    /// `(hits, misses)` since the last reset.
    pub fn cache_stats(&self) -> (u64, u64) {
        use std::sync::atomic::Ordering::Relaxed;
        (self.hits.load(Relaxed), self.misses.load(Relaxed))
    }

    pub fn reset_stats(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        self.hits.store(0, Relaxed);
        self.misses.store(0, Relaxed);
    }

    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// Touch the memory map so the first real lookup does not pay for hundreds
    /// of page faults — measured at ~490 ms for Jitendex, which is exactly the
    /// kind of delay a user notices on the very first hover.
    ///
    /// The probes are common function words, which sit in different parts of the
    /// index and between them pull in the hot pages.
    pub fn warm_up(&self) {
        for probe in ["の", "する", "た", "い", "こと", "ある", "私", "して"] {
            let _ = self.lookup_uncached(probe, 4);
        }
        self.clear_cache();
        self.reset_stats();
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}
