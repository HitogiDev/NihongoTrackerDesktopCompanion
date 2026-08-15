use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use nihongo_dict::{Dictionary, LookupRow};

/// A handful of engines, handed out round-robin.
///
/// The C++ engine is not thread-safe, so one instance serialises every lookup
/// behind its mutex. At the measured 2.6 ms per offset that is about 380
/// lookups a second, which is fine for one reader and thin for a shared
/// service — so the service holds several. They cost little: the dictionary is
/// a read-only memory map, and the kernel shares those pages between them.
///
/// There is no result cache on purpose. The spike measured a 7.4 % hit rate for
/// one keyed on the input text, because every offset of a line is a different
/// suffix; caching here would spend memory to save almost nothing.
pub struct DictionaryPool {
    engines: Vec<Dictionary>,
    next: AtomicUsize,
    loaded: Vec<LoadedDictionary>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LoadedDictionary {
    pub name: String,
    pub path: String,
}

impl DictionaryPool {
    /// Opens `size` engines over the same dictionaries and warms every one of
    /// them, so the first request never pays the ~490 ms of page faults the
    /// spike measured on a cold memory map.
    pub fn open(dictionaries: &[PathBuf], size: usize) -> Self {
        let size = size.clamp(1, 16);
        let mut engines = Vec::with_capacity(size);

        for _ in 0..size {
            let dictionary = Dictionary::new();
            for path in dictionaries {
                dictionary.add_term_dict(&path.to_string_lossy());
                // Frequency dictionaries live in the same folder shape; adding a
                // term dictionary as a frequency source is harmless when it has
                // no frequency data, and it is what makes `frequencies` arrive
                // populated for the ones that do.
                dictionary.add_freq_dict(&path.to_string_lossy());
            }
            dictionary.warm_up();
            engines.push(dictionary);
        }

        let loaded = dictionaries
            .iter()
            .map(|path| LoadedDictionary {
                name: dictionary_name(path),
                path: path.to_string_lossy().to_string(),
            })
            .collect();

        Self {
            engines,
            next: AtomicUsize::new(0),
            loaded,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    pub fn loaded(&self) -> &[LoadedDictionary] {
        &self.loaded
    }

    pub fn size(&self) -> usize {
        self.engines.len()
    }

    /// One lookup, on whichever engine comes next.
    pub fn lookup(&self, text: &str, max_results: i32) -> Vec<LookupRow> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.engines.len();
        self.engines[index].lookup_uncached(text, max_results)
    }
}

/// The folder an import produced is named after the dictionary's own title.
fn dictionary_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
