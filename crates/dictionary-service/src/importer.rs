use std::path::{Path, PathBuf};

/// Import any `.zip` in `import_dir` that has not been imported yet, and return
/// every imported dictionary found in `data_dir`.
///
/// `data_dir` is a volume, so this is a no-op on every start after the first —
/// importing Jitendex takes about 11 seconds and produces 181 MB, and doing it
/// on each boot would be both slow and pointless.
pub fn sync_dictionaries(import_dir: &Path, data_dir: &Path) -> Vec<PathBuf> {
    if let Err(error) = std::fs::create_dir_all(data_dir) {
        tracing::error!(?error, path = %data_dir.display(), "cannot create the dictionary data directory");
        return Vec::new();
    }

    for zip in zips_in(import_dir) {
        match already_imported(&zip, data_dir) {
            Some(existing) => {
                tracing::info!(zip = %zip.display(), imported = %existing.display(), "already imported, skipping");
            }
            None => import_one(&zip, data_dir),
        }
    }

    imported_dictionaries(data_dir)
}

fn zips_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip")))
        .collect()
}

/// An import is identified by the dictionary's own title, which is not the zip's
/// file name — so the check is "is there an imported dictionary whose folder
/// looks like it came from this file", done by marker file rather than by
/// guessing at names.
fn already_imported(zip: &Path, data_dir: &Path) -> Option<PathBuf> {
    let marker_name = format!(".imported-{}", file_stem(zip));
    let marker = data_dir.join(&marker_name);
    let target = std::fs::read_to_string(&marker).ok()?;
    let path = data_dir.join(target.trim());
    path.join("index.json").exists().then_some(path)
}

fn import_one(zip: &Path, data_dir: &Path) {
    tracing::info!(zip = %zip.display(), "importing dictionary");
    let started = std::time::Instant::now();
    let summary = nihongo_dict::import(&zip.to_string_lossy(), &data_dir.to_string_lossy());

    if !summary.success {
        tracing::error!(zip = %zip.display(), errors = ?summary.errors, "import failed");
        return;
    }

    tracing::info!(
        title = %summary.title,
        terms = summary.term_count,
        seconds = started.elapsed().as_secs_f64(),
        "imported"
    );

    // Remember what this zip produced, so the next boot skips it.
    let marker = data_dir.join(format!(".imported-{}", file_stem(zip)));
    if let Err(error) = std::fs::write(&marker, &summary.title) {
        tracing::warn!(?error, "could not write the import marker; this zip will be imported again");
    }
}

fn imported_dictionaries(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(data_dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        // An imported dictionary is a directory with an index.json in it.
        .filter(|path| path.is_dir() && path.join("index.json").exists())
        .collect();
    found.sort();
    found
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "dictionary".to_string())
}
