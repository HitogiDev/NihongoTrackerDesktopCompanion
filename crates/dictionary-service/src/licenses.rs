use std::path::Path;

use serde::{Deserialize, Serialize};

/// Attribution for one loaded dictionary.
///
/// This is not decoration. Jitendex is CC BY-SA 4.0 over JMdict and Tatoeba, and
/// the moment we serve its content from our own product the attribution becomes
/// our obligation rather than the user's — so the service ships it next to the
/// data and the clients have no excuse not to show it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DictionaryLicense {
    /// The dictionary title, as the importer recorded it.
    pub name: String,
    pub license: String,
    pub url: Option<String>,
    /// Upstream sources the dictionary itself is derived from.
    #[serde(default)]
    pub sources: Vec<LicenseSource>,
    /// Shown verbatim by the clients, so it says what it needs to say.
    pub attribution: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LicenseSource {
    pub name: String,
    pub license: String,
    pub url: Option<String>,
}

/// What we know without being told. Matched on a prefix because the imported
/// title carries a date — `Jitendex.org [2026-08-11]`.
fn known(name: &str) -> Option<DictionaryLicense> {
    if name.starts_with("Jitendex") {
        return Some(DictionaryLicense {
            name: name.to_string(),
            license: "CC BY-SA 4.0".into(),
            url: Some("https://jitendex.org".into()),
            sources: vec![
                LicenseSource {
                    name: "JMdict".into(),
                    license: "CC BY-SA 4.0".into(),
                    url: Some("https://www.edrdg.org/edrdg/licence.html".into()),
                },
                LicenseSource {
                    name: "Tatoeba".into(),
                    license: "CC BY 2.0 FR".into(),
                    url: Some("https://tatoeba.org".into()),
                },
            ],
            attribution:
                "Definitions from Jitendex (CC BY-SA 4.0), derived from JMdict by the Electronic \
                 Dictionary Research and Development Group (CC BY-SA 4.0), with example sentences \
                 from Tatoeba (CC BY 2.0 FR)."
                    .into(),
        });
    }
    None
}

/// Licenses for the dictionaries actually loaded.
///
/// An operator who adds a dictionary we have no entry for gets a placeholder
/// saying exactly that, rather than silence — an unattributed dictionary is a
/// problem someone has to see.
pub fn resolve(names: &[String], data_dir: &Path) -> Vec<DictionaryLicense> {
    let overrides = load_overrides(data_dir);

    names
        .iter()
        .map(|name| {
            if let Some(found) = overrides.iter().find(|entry| entry.name == *name) {
                return found.clone();
            }
            known(name).unwrap_or_else(|| DictionaryLicense {
                name: name.clone(),
                license: "unknown".into(),
                url: None,
                sources: Vec::new(),
                attribution: format!(
                    "No licence on record for {name}. Add it to licenses.json in the dictionary \
                     data directory before serving this dictionary publicly."
                ),
            })
        })
        .collect()
}

/// `licenses.json` in the data volume, for dictionaries beyond the ones we know.
fn load_overrides(data_dir: &Path) -> Vec<DictionaryLicense> {
    let path = data_dir.join("licenses.json");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<DictionaryLicense>>(&raw) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(?error, path = %path.display(), "ignoring malformed licenses.json");
            Vec::new()
        }
    }
}
