//! Offline Yomitan lookup backed by Hoshidicts main.
//!
//! Hoshidicts converts imported Yomitan archives into memory-mappable files and
//! performs substring lookup, preprocessing, and Japanese deinflection offline.
//! The companion keeps a small manifest beside each converted dictionary so the
//! app can rebuild the native query engine after restart without touching the
//! original archive again.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nihongo_dict::{self, FreqHit, ImportSummary, LookupRow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const MAX_QUERY_CHARS: usize = 32;
const DEFAULT_MAX_RESULTS: i32 = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionarySummary {
    pub id: String,
    pub title: String,
    pub revision: String,
    pub language: String,
    pub term_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryFrequency {
    pub dictionary: String,
    pub display: String,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub dictionary: String,
    pub expression: String,
    pub reading: String,
    pub tags: String,
    pub score: i64,
    pub matched: String,
    pub deinflected: String,
    pub process: Vec<String>,
    pub glossaries: Vec<Value>,
    pub frequencies: Vec<DictionaryFrequency>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResponse {
    pub query: String,
    pub matched: String,
    pub entries: Vec<DictionaryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDictionary {
    summary: DictionarySummary,
    engine_path: String,
    meta_count: usize,
    freq_count: usize,
    pitch_count: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EngineIndex {
    revision: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
}

struct StoreInner {
    dictionaries: Vec<PersistedDictionary>,
    engine: nihongo_dict::Dictionary,
}

pub struct DictionaryStore {
    root: PathBuf,
    inner: Mutex<StoreInner>,
}

impl DictionaryStore {
    pub fn open(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        let store = Self {
            root,
            inner: Mutex::new(StoreInner {
                dictionaries: Vec::new(),
                engine: nihongo_dict::Dictionary::new(),
            }),
        };
        store.migrate_legacy_archives();
        store.reload();
        store
    }

    pub fn list(&self) -> Vec<DictionarySummary> {
        self.inner
            .lock()
            .expect("dictionary mutex poisoned")
            .dictionaries
            .iter()
            .map(|dictionary| dictionary.summary.clone())
            .collect()
    }

    pub fn import_zip(&self, source: &Path) -> Result<DictionarySummary, String> {
        validate_zip(source)?;

        let id = format!(
            "{}-{}",
            slug(
                source
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("dictionary")
            ),
            Uuid::new_v4()
        );
        let destination = self.root.join(&id);
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;

        let imported =
            nihongo_dict::import(&source.to_string_lossy(), &destination.to_string_lossy());
        if !imported.success {
            let _ = fs::remove_dir_all(&destination);
            return Err(if imported.error.is_empty() {
                "Hoshidicts could not import this Yomitan archive".into()
            } else {
                imported.error
            });
        }

        let engine_path = find_engine_directory(&destination)?;
        let persisted = persisted_dictionary(&id, &destination, &engine_path, &imported)?;
        write_manifest(&destination, &persisted)?;

        self.reload();
        Ok(persisted.summary)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        if !is_safe_id(id) {
            return Err("dictionary id is invalid".into());
        }
        let path = self.root.join(id);
        if !path.is_dir() {
            return Err("dictionary not found".into());
        }
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        self.reload();
        Ok(())
    }

    pub fn lookup(
        &self,
        text: &str,
        offset: usize,
        max_results: usize,
    ) -> Result<LookupResponse, String> {
        let start = byte_offset_for_utf16(text, offset)
            .ok_or_else(|| "lookup offset is outside the text".to_string())?;
        let raw_suffix = &text[start..];
        let suffix = raw_suffix
            .char_indices()
            .find(|(_, character)| is_lookup_character(*character))
            .map(|(index, _)| &raw_suffix[index..])
            .unwrap_or(raw_suffix);

        if suffix.trim().is_empty() {
            return Ok(LookupResponse {
                query: text.to_string(),
                matched: String::new(),
                entries: Vec::new(),
            });
        }

        let rows = self
            .inner
            .lock()
            .expect("dictionary mutex poisoned")
            .engine
            .lookup(
                suffix,
                max_results.clamp(1, 32).min(DEFAULT_MAX_RESULTS as usize) as i32,
            );
        let matched = rows
            .first()
            .map(|row| row.matched.clone())
            .unwrap_or_default();
        let entries = rows.into_iter().flat_map(entries_from_row).collect();

        Ok(LookupResponse {
            query: suffix.chars().take(MAX_QUERY_CHARS).collect(),
            matched,
            entries,
        })
    }

    fn reload(&self) {
        let dictionaries = read_manifests(&self.root);
        let mut inner = self.inner.lock().expect("dictionary mutex poisoned");
        inner.engine = nihongo_dict::Dictionary::new();
        for dictionary in &dictionaries {
            let path = self.root.join(&dictionary.engine_path);
            inner.engine.add_term_dict(&path.to_string_lossy());
            if dictionary.freq_count > 0 {
                inner.engine.add_freq_dict(&path.to_string_lossy());
            }
            if dictionary.pitch_count > 0 {
                inner.engine.add_pitch_dict(&path.to_string_lossy());
            }
        }
        inner.dictionaries = dictionaries;
    }

    fn migrate_legacy_archives(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            {
                continue;
            }
            let id = format!(
                "{}-{}",
                slug(
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("dictionary")
                ),
                Uuid::new_v4()
            );
            let destination = self.root.join(&id);
            if fs::create_dir_all(&destination).is_err() {
                continue;
            }
            let imported =
                nihongo_dict::import(&path.to_string_lossy(), &destination.to_string_lossy());
            if !imported.success {
                let _ = fs::remove_dir_all(destination);
                continue;
            }
            let Ok(engine_path) = find_engine_directory(&destination) else {
                continue;
            };
            let Ok(persisted) = persisted_dictionary(&id, &destination, &engine_path, &imported)
            else {
                continue;
            };
            if write_manifest(&destination, &persisted).is_err() {
                continue;
            }
            let legacy_path = self.root.join(format!("{id}.legacy"));
            let _ = fs::rename(path, legacy_path);
        }
    }
}

fn validate_zip(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("dictionary file does not exist".into());
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Yomitan dictionaries must be .zip files".into());
    }
    Ok(())
}

fn persisted_dictionary(
    id: &str,
    destination: &Path,
    engine_path: &Path,
    imported: &ImportSummary,
) -> Result<PersistedDictionary, String> {
    let index_path = engine_path.join("index.json");
    let index = fs::read_to_string(&index_path)
        .map_err(|error| format!("Hoshidicts created no readable index: {error}"))
        .and_then(|raw| {
            serde_json::from_str::<EngineIndex>(&raw)
                .map_err(|error| format!("invalid Hoshidicts index: {error}"))
        })?;
    let language = index
        .source_language
        .or(index.target_language)
        .unwrap_or_else(|| "日本語".into());
    Ok(PersistedDictionary {
        summary: DictionarySummary {
            id: id.into(),
            title: imported.title.clone(),
            revision: index.revision.unwrap_or_default(),
            language,
            term_count: imported.term_count as usize,
        },
        engine_path: engine_path
            .strip_prefix(destination.parent().unwrap_or(destination))
            .unwrap_or(engine_path)
            .to_string_lossy()
            .replace('\\', "/"),
        meta_count: imported.meta_count as usize,
        freq_count: imported.freq_count as usize,
        pitch_count: imported.pitch_count as usize,
    })
}

fn write_manifest(destination: &Path, dictionary: &PersistedDictionary) -> Result<(), String> {
    let manifest = serde_json::to_vec_pretty(dictionary).map_err(|error| error.to_string())?;
    fs::write(destination.join("manifest.json"), manifest).map_err(|error| error.to_string())
}

fn read_manifests(root: &Path) -> Vec<PersistedDictionary> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dictionaries = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let raw = fs::read(path.join("manifest.json")).ok()?;
            serde_json::from_slice::<PersistedDictionary>(&raw).ok()
        })
        .collect::<Vec<_>>();
    dictionaries.sort_by(|left, right| left.summary.title.cmp(&right.summary.title));
    dictionaries
}

fn find_engine_directory(root: &Path) -> Result<PathBuf, String> {
    if root.join(".hoshidicts_3").is_file() || root.join(".hoshidicts_4").is_file() {
        return Ok(root.to_path_buf());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        if entry.path().is_dir() {
            if let Ok(path) = find_engine_directory(&entry.path()) {
                return Ok(path);
            }
        }
    }
    Err("Hoshidicts created no usable dictionary directory".into())
}

fn entries_from_row(row: LookupRow) -> Vec<DictionaryEntry> {
    let mut glossaries_by_dictionary = BTreeMap::<String, Vec<Value>>::new();
    for gloss in row.glossaries {
        glossaries_by_dictionary
            .entry(gloss.dict_name)
            .or_default()
            .push(serde_json::from_str(&gloss.text).unwrap_or(Value::String(gloss.text)));
    }

    let tags = [row.definition_tags, row.rules, row.term_tags]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let frequencies = row
        .frequencies
        .into_iter()
        .map(frequency_from_hit)
        .collect::<Vec<_>>();

    glossaries_by_dictionary
        .into_iter()
        .map(|(dictionary, glossaries)| DictionaryEntry {
            frequencies: frequencies
                .iter()
                .filter(|frequency| frequency.dictionary == dictionary)
                .cloned()
                .collect(),
            dictionary,
            expression: row.expression.clone(),
            reading: row.reading.clone(),
            tags: tags.clone(),
            score: row.score as i64,
            matched: row.matched.clone(),
            deinflected: row.deinflected.clone(),
            process: row.process.clone(),
            glossaries,
        })
        .collect()
}

fn frequency_from_hit(hit: FreqHit) -> DictionaryFrequency {
    DictionaryFrequency {
        dictionary: hit.dict_name,
        display: hit.display,
        value: hit.value,
    }
}

fn slug(value: &str) -> String {
    let result: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let result: String = result.trim_matches('-').chars().take(48).collect();
    if result.is_empty() {
        "dictionary".into()
    } else {
        result
    }
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && Path::new(id).file_name().is_some_and(|name| name == id)
}

fn byte_offset_for_utf16(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 {
        return Some(0);
    }
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units == offset {
            return Some(byte);
        }
        units += character.len_utf16();
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(text.len())
}

fn is_lookup_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{byte_offset_for_utf16, DictionaryStore};
    use uuid::Uuid;

    #[test]
    fn utf16_offsets_land_on_character_boundaries() {
        assert_eq!(byte_offset_for_utf16("あ😀本", 0), Some(0));
        assert_eq!(byte_offset_for_utf16("あ😀本", 1), Some(3));
        assert_eq!(byte_offset_for_utf16("あ😀本", 3), Some(7));
        assert_eq!(byte_offset_for_utf16("あ😀本", 4), Some(10));
        assert_eq!(byte_offset_for_utf16("あ😀本", 2), None);
    }

    #[test]
    fn imports_yomitan_zip_and_uses_hoshidicts_deinflection() {
        let root = std::env::temp_dir().join(format!("nt-dictionary-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("sample.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("index.json", options).unwrap();
        archive.write_all(br#"{"title":"Test Jiten","revision":"1","format":3,"sourceLanguage":"ja","targetLanguage":"en"}"#).unwrap();
        archive.start_file("term_bank_1.json", options).unwrap();
        archive
            .write_all(r#"[["食べる","たべる","","v1",0,["to eat"],1,""]]"#.as_bytes())
            .unwrap();
        archive.finish().unwrap();

        let store = DictionaryStore::open(root.join("dictionaries"));
        let summary = store.import_zip(&zip_path).unwrap();
        assert_eq!(summary.title, "Test Jiten");
        assert_eq!(summary.term_count, 1);

        let result = store.lookup("食べました", 0, 12).unwrap();
        assert_eq!(result.matched, "食べました");
        assert_eq!(result.entries[0].expression, "食べる");
        assert!(!result.entries[0].process.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }
}
