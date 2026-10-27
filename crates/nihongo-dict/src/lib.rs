//! Small safe Rust wrapper around the Hoshidicts main C++ engine.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use lru::LruCache;

#[cxx::bridge(namespace = "ntdict")]
pub mod ffi {
    #[derive(Debug, Clone)]
    pub struct ImportSummary {
        pub success: bool,
        pub title: String,
        pub term_count: u64,
        pub meta_count: u64,
        pub freq_count: u64,
        pub pitch_count: u64,
        pub error: String,
    }

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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LookupRow {
        pub matched: String,
        pub deinflected: String,
        pub process: Vec<String>,
        pub expression: String,
        pub reading: String,
        pub definition_tags: String,
        pub rules: String,
        pub term_tags: String,
        pub score: i32,
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
        fn add_pitch_dict(self: Pin<&mut Engine>, path: &str);
        fn lookup(&self, text: &str, max_results: i32) -> Vec<LookupRow>;
    }
}

pub use ffi::{FreqHit, Gloss, ImportSummary, LookupRow};

const CACHE_CAPACITY: usize = 4096;
const SCAN_LENGTH: usize = 16;

pub struct Dictionary {
    engine: Mutex<cxx::UniquePtr<ffi::Engine>>,
    cache: Mutex<LruCache<(String, i32), Vec<LookupRow>>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

unsafe impl Send for Dictionary {}
unsafe impl Sync for Dictionary {}

impl Dictionary {
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(ffi::new_engine()),
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(CACHE_CAPACITY).expect("non-zero cache"),
            )),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    pub fn add_term_dict(&self, path: &str) {
        self.engine
            .lock()
            .expect("dictionary mutex poisoned")
            .pin_mut()
            .add_term_dict(path);
        self.clear_cache();
    }

    pub fn add_freq_dict(&self, path: &str) {
        self.engine
            .lock()
            .expect("dictionary mutex poisoned")
            .pin_mut()
            .add_freq_dict(path);
        self.clear_cache();
    }

    pub fn add_pitch_dict(&self, path: &str) {
        self.engine
            .lock()
            .expect("dictionary mutex poisoned")
            .pin_mut()
            .add_pitch_dict(path);
        self.clear_cache();
    }

    pub fn lookup(&self, text: &str, max_results: i32) -> Vec<LookupRow> {
        let key = (text.chars().take(SCAN_LENGTH).collect(), max_results);
        if let Some(value) = self
            .cache
            .lock()
            .expect("dictionary cache poisoned")
            .get(&key)
        {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return value.clone();
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let value = self
            .engine
            .lock()
            .expect("dictionary mutex poisoned")
            .lookup(text, max_results);
        self.cache
            .lock()
            .expect("dictionary cache poisoned")
            .put(key, value.clone());
        value
    }

    pub fn clear_cache(&self) {
        self.cache
            .lock()
            .expect("dictionary cache poisoned")
            .clear();
    }

    pub fn cache_stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

pub fn import(zip_path: &str, out_dir: &str) -> ImportSummary {
    ffi::import_dictionary(zip_path, out_dir)
}
