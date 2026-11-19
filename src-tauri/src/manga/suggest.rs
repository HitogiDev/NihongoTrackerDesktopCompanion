//! Link suggestions: for each series that is not linked to NihongoTracker, a
//! background search of its title (`sync::spawn_manga_suggester`) and the
//! media that looks most like it, kept as a **suggestion**. Nothing here
//! links a series: the user confirms (then it is `link_manga_series`, like a
//! link picked by hand) or dismisses, and a dismissed media is never
//! suggested again for that series.
//!
//! Everything in this file is pure or database work: titles are cleaned
//! (`clean_title`), hits scored (`score`, `best_match`) and the loop paced
//! (`Pace`) without the network, so the tests need no server.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::api::MediaHit;
use crate::db::{now_iso, Db, Result};

/// Told to the page when suggestions change.
pub const SUGGESTIONS_CHANGED: &str = "manga://suggestions";
/// The media type looked up.
pub const SEARCH_TYPE: &str = "manga";
/// Hits asked for per search.
pub const SEARCH_HITS: u32 = 8;
/// Below this a hit is not worth showing: a wrong suggestion costs the user
/// more than none.
pub const MIN_SCORE: f64 = 0.6;
/// The series the page says are on screen, at most.
pub const PRIORITY_LIMIT: usize = 240;
/// A search that found nothing is tried again after this long: the media
/// may have been added since.
pub const RETRY_EMPTY_AFTER_DAYS: i64 = 30;

/// A suggestion waiting for the user, as the UI shows it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct MangaLinkSuggestion {
    pub series_id: String,
    /// The cleaned title that was searched.
    pub query: String,
    pub media_id: String,
    /// The title the link would carry (native, else romaji), as a manual link.
    pub media_title: String,
    /// The romaji title, when it is not the one above.
    pub media_subtitle: Option<String>,
    pub media_image: Option<String>,
    pub is_adult: bool,
    /// How much the media's titles look like the series' (0 to 1).
    pub score: f64,
}

fn noise() -> &'static [Regex] {
    static NOISE: OnceLock<Vec<Regex>> = OnceLock::new();
    NOISE.get_or_init(|| {
        [
            // A volume or chapter marker and anything after it: "Vol. 3",
            // "Volume 12", "v01", "Tome 2", "Ch. 5", "#12" (not "#Zombie").
            r"(?i)(?:^|[\s_\-–—,.])(?:vol(?:ume)?s?|tomo|tome|ch(?:apter)?|ep)\.?\s*\d+.*$",
            r"(?i)(?:^|[\s_\-–—])v\d{1,3}(?:\s*-\s*v?\d{1,3})?\b.*$",
            r"(?:^|\s)#\d+.*$",
            // A Japanese volume number: "第3巻", "3巻".
            r"\s*第?\s*[0-9０-９]+\s*巻.*$",
            // A range of volumes at the end: "01-15", "1~3".
            r"[\s_\-–—]+\d{1,4}\s*[-~〜]\s*\d{1,4}\s*$",
            // A lone number after a dash at the end: "Title - 03".
            r"\s+[-–—]\s*\d{1,4}\s*$",
        ]
        .iter()
        .map(|pattern| Regex::new(pattern).expect("noise pattern"))
        .collect()
    })
}

fn brackets() -> &'static Regex {
    static BRACKETS: OnceLock<Regex> = OnceLock::new();
    BRACKETS.get_or_init(|| Regex::new(r"\([^()]*\)|\[[^\[\]]*\]|\{[^{}]*\}|【[^【】]*】|（[^（）]*）|「[^「」]*」").expect("bracket pattern"))
}

/// Full-width ASCII as plain ASCII ("ＡＢＣ１" → "ABC1"), and the ideographic
/// space as a space.
fn half_width(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

/// A folder name as a title to search: no release tags in brackets
/// ("(Upscaled)", "[HD Scan]", "【完結】"), no volume numbers, no dots or
/// underscores for spaces. A title that is all brackets ("[Oshi no Ko]")
/// keeps what is inside them.
pub fn clean_title(raw: &str) -> String {
    let mut text = half_width(raw.trim());
    // Dots and underscores between words are a file name's spaces.
    if !text.contains(' ') && (text.contains('_') || text.matches('.').count() > 1) {
        text = text.replace(['_', '.'], " ");
    }
    text = text.replace('_', " ");
    let without = brackets().replace_all(&text, " ").to_string();
    let mut text = if without.trim().is_empty() {
        // Only brackets: what they hold is the title.
        brackets()
            .find_iter(&text)
            .map(|m| {
                let inner = m.as_str();
                let mut chars = inner.chars();
                chars.next();
                chars.next_back();
                chars.as_str().to_string()
            })
            .next()
            .unwrap_or_default()
    } else {
        without
    };
    for pattern in noise() {
        let next = pattern.replace(&text, "").to_string();
        // A title that is only a marker ("Vol. 1") is better than nothing.
        if !next.trim().is_empty() {
            text = next;
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.trim_matches(|c: char| matches!(c, '-' | '–' | '—' | ',' | '.' | ':' | ';' | '_' | '~' | '|') || c.is_whitespace())
        .to_string()
}

/// What two titles are compared as: letters and digits only (kana and kanji
/// too), lower case, full width folded.
pub fn normalize(title: &str) -> String {
    half_width(title).chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Dice's coefficient over character pairs: 1 for the same string, 0 for
/// nothing in common. Single characters compare as themselves.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (normalize(a), normalize(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let pairs = |text: &str| -> Vec<(char, char)> {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() == 1 {
            return vec![(chars[0], '\0')];
        }
        chars.windows(2).map(|pair| (pair[0], pair[1])).collect()
    };
    let left = pairs(&a);
    let mut right = pairs(&b);
    let total = left.len() + right.len();
    let mut shared = 0;
    for pair in &left {
        if let Some(index) = right.iter().position(|other| other == pair) {
            right.swap_remove(index);
            shared += 1;
        }
    }
    (2 * shared) as f64 / total as f64
}

/// Every title a hit goes by.
fn hit_titles(hit: &MediaHit) -> Vec<&str> {
    let mut titles = vec![hit.title.native.as_str()];
    titles.extend(hit.title.romaji.as_deref());
    titles.extend(hit.title.english.as_deref());
    titles.extend(hit.synonyms.iter().flatten().map(String::as_str));
    titles.into_iter().filter(|title| !title.trim().is_empty()).collect()
}

/// How well a hit matches the searched title: its best title.
pub fn score(query: &str, hit: &MediaHit) -> f64 {
    hit_titles(hit).into_iter().map(|title| similarity(query, title)).fold(0.0, f64::max)
}

/// The hit to suggest: the best scored above `MIN_SCORE` that the user has
/// not dismissed for this series (ties go to the search's own order).
pub fn best_match<'a>(query: &str, hits: &'a [MediaHit], dismissed: &HashSet<String>) -> Option<(&'a MediaHit, f64)> {
    let mut best: Option<(&MediaHit, f64)> = None;
    for hit in hits {
        if hit.content_id.trim().is_empty() || dismissed.contains(&hit.content_id) {
            continue;
        }
        let value = score(query, hit);
        if value >= MIN_SCORE && best.is_none_or(|(_, current)| value > current) {
            best = Some((hit, value));
        }
    }
    best
}

/// How a search went, for the pace of the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Answered (with hits or without).
    Searched,
    /// Answered from the cache: the network was not used.
    Cached,
    /// 429: the server asks to slow down.
    RateLimited,
    /// The key was refused: no point in trying again soon.
    Unauthorized,
    /// No network, or the server failed.
    Failed,
    /// Nothing to look up, or suggestions are off.
    Idle,
}

/// The rhythm of the background searches: one every `INTERVAL` at most, and
/// longer and longer pauses while the server refuses or fails.
#[derive(Debug, Default)]
pub struct Pace {
    backoff: Option<Duration>,
}

impl Pace {
    pub const INTERVAL: Duration = Duration::from_millis(1500);
    pub const IDLE: Duration = Duration::from_secs(600);
    const FIRST_RATE_LIMIT: Duration = Duration::from_secs(30);
    const FIRST_FAILURE: Duration = Duration::from_secs(15);
    const MAX_BACKOFF: Duration = Duration::from_secs(600);
    const UNAUTHORIZED: Duration = Duration::from_secs(1800);

    /// How long to wait before the next step.
    pub fn after(&mut self, outcome: Outcome) -> Duration {
        match outcome {
            Outcome::Searched => {
                self.backoff = None;
                Self::INTERVAL
            }
            Outcome::Cached => Duration::ZERO,
            Outcome::Idle => Self::IDLE,
            Outcome::Unauthorized => Self::UNAUTHORIZED,
            Outcome::RateLimited | Outcome::Failed => {
                let first = if outcome == Outcome::RateLimited { Self::FIRST_RATE_LIMIT } else { Self::FIRST_FAILURE };
                let next = self.backoff.map_or(first, |current| (current * 2).max(first)).min(Self::MAX_BACKOFF);
                self.backoff = Some(next);
                next
            }
        }
    }

    /// Whether a nudge (the user scrolled, settings saved) may cut the wait
    /// short: only when there was nothing to do, never a backoff.
    pub fn interruptible(outcome: Outcome) -> bool {
        outcome == Outcome::Idle
    }
}

impl Db {
    /// The next unlinked series to look up: those on screen first (in the
    /// order given), then the most recently read, then by title. A series
    /// already looked up is skipped, unless its search found nothing a
    /// while ago.
    pub fn next_suggestion_candidate(&self, priority: &[String]) -> Result<Option<(String, String)>> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(RETRY_EMPTY_AFTER_DAYS)).to_rfc3339();
        self.with(|c| {
            let pending = "s.media_id IS NULL AND NOT EXISTS (
                    SELECT 1 FROM manga_link_suggestions g
                    WHERE g.series_id = s.id AND (g.media_id IS NOT NULL OR g.checked_at > ?1))";
            if !priority.is_empty() {
                let mut stmt = c.prepare(&format!("SELECT s.id, s.title FROM manga_series s WHERE s.id = ?2 AND {pending}"))?;
                for id in priority {
                    if let Some(row) = stmt.query_row(params![cutoff, id], |r| Ok((r.get(0)?, r.get(1)?))).optional()? {
                        return Ok(Some(row));
                    }
                }
            }
            c.query_row(
                &format!(
                    "SELECT s.id, s.title FROM manga_series s
                     LEFT JOIN (SELECT v.series_id, MAX(p.updated_at) AS read_at
                                FROM manga_volumes v JOIN manga_progress p ON p.volume_id = v.id
                                GROUP BY v.series_id) r ON r.series_id = s.id
                     WHERE {pending}
                     ORDER BY r.read_at IS NULL, r.read_at DESC, s.title COLLATE NOCASE
                     LIMIT 1"
                ),
                params![cutoff],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
        })
    }

    /// The hits a cleaned title got before, if it was searched.
    pub fn cached_title_search(&self, query: &str) -> Result<Option<Vec<MediaHit>>> {
        let raw: Option<String> = self.with(|c| {
            c.query_row("SELECT hits_json FROM manga_title_search WHERE query = ?1", params![normalize(query)], |r| r.get(0)).optional()
        })?;
        Ok(raw.map(|raw| serde_json::from_str::<Vec<MediaHit>>(&raw).unwrap_or_default()))
    }

    pub fn cache_title_search(&self, query: &str, hits: &[MediaHit]) -> Result<()> {
        let json = serde_json::to_string(hits).unwrap_or_else(|_| "[]".into());
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_title_search (query, hits_json, searched_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(query) DO UPDATE SET hits_json = excluded.hits_json, searched_at = excluded.searched_at",
                params![normalize(query), json, now_iso()],
            )
            .map(|_| ())
        })
    }

    /// The media dismissed for a series.
    pub fn dismissed_suggestions(&self, series_id: &str) -> Result<HashSet<String>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT media_id FROM manga_link_dismissed WHERE series_id = ?1")?;
            let rows = stmt.query_map(params![series_id], |r| r.get::<_, String>(0))?;
            rows.collect()
        })
    }

    /// Keeps what the search of a series came to: a suggestion, or `None`
    /// (looked up, nothing good enough).
    pub fn set_link_suggestion(&self, series_id: &str, query: &str, best: Option<(&MediaHit, f64)>) -> Result<()> {
        let (media_id, title, subtitle, image, adult, value) = match best {
            Some((hit, value)) => {
                let native = hit.title.native.trim();
                let romaji = hit.title.romaji.as_deref().map(str::trim).filter(|romaji| !romaji.is_empty());
                let title = if native.is_empty() { romaji.unwrap_or(hit.content_id.as_str()).to_string() } else { native.to_string() };
                let subtitle = romaji.filter(|romaji| *romaji != title).map(str::to_string);
                let image = hit.content_image.clone().or_else(|| hit.cover_image.clone()).filter(|url| url.starts_with("https://") || url.starts_with("http://"));
                (Some(hit.content_id.clone()), Some(title), subtitle, image, hit.is_adult, value)
            }
            None => (None, None, None, None, false, 0.0),
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO manga_link_suggestions (series_id, query, media_id, media_title, media_subtitle, media_image, is_adult, score, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(series_id) DO UPDATE SET query = excluded.query, media_id = excluded.media_id,
                    media_title = excluded.media_title, media_subtitle = excluded.media_subtitle,
                    media_image = excluded.media_image, is_adult = excluded.is_adult, score = excluded.score,
                    checked_at = excluded.checked_at",
                params![series_id, query, media_id, title, subtitle, image, adult as i64, value, now_iso()],
            )
            .map(|_| ())
        })
    }

    /// The suggestions waiting for the user: series still unlinked, with a media.
    pub fn link_suggestions(&self) -> Result<Vec<MangaLinkSuggestion>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT g.series_id, g.query, g.media_id, g.media_title, g.media_subtitle, g.media_image, g.is_adult, g.score
                 FROM manga_link_suggestions g JOIN manga_series s ON s.id = g.series_id
                 WHERE g.media_id IS NOT NULL AND s.media_id IS NULL
                 ORDER BY g.score DESC, s.title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(MangaLinkSuggestion {
                    series_id: r.get(0)?,
                    query: r.get(1)?,
                    media_id: r.get(2)?,
                    media_title: r.get(3)?,
                    media_subtitle: r.get(4)?,
                    media_image: r.get(5)?,
                    is_adult: r.get::<_, i64>(6)? != 0,
                    score: r.get(7)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn link_suggestion(&self, series_id: &str) -> Result<Option<MangaLinkSuggestion>> {
        Ok(self.link_suggestions()?.into_iter().find(|suggestion| suggestion.series_id == series_id))
    }

    /// "Not this one": never suggested again for this series. The next best
    /// hit of the same search (from the cache, no network) takes its place.
    pub fn dismiss_link_suggestion(&self, series_id: &str) -> Result<bool> {
        let Some(current) = self.link_suggestion(series_id)? else { return Ok(false) };
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO manga_link_dismissed (series_id, media_id, dismissed_at) VALUES (?1, ?2, ?3)",
                params![series_id, current.media_id, now_iso()],
            )
            .map(|_| ())
        })?;
        let dismissed = self.dismissed_suggestions(series_id)?;
        let hits = self.cached_title_search(&current.query)?.unwrap_or_default();
        self.set_link_suggestion(series_id, &current.query, best_match(&current.query, &hits, &dismissed))?;
        Ok(true)
    }
}

/// One step of the loop without the network: the next series, and its
/// search if the cache has it. `Search` asks the caller to search `query`
/// for `series_id` and come back with `finish`.
pub enum Step {
    Nothing,
    Done { series_id: String },
    Search { series_id: String, query: String },
}

pub fn prepare(db: &Db, priority: &[String]) -> Result<Step> {
    let Some((series_id, title)) = db.next_suggestion_candidate(priority)? else { return Ok(Step::Nothing) };
    let query = clean_title(&title);
    if normalize(&query).is_empty() {
        db.set_link_suggestion(&series_id, &query, None)?;
        return Ok(Step::Done { series_id });
    }
    match db.cached_title_search(&query)? {
        Some(hits) => {
            finish(db, &series_id, &query, &hits)?;
            Ok(Step::Done { series_id })
        }
        None => Ok(Step::Search { series_id, query }),
    }
}

/// Keeps a search's hits (for every series with the same clean title) and
/// the suggestion they make for this series.
pub fn finish(db: &Db, series_id: &str, query: &str, hits: &[MediaHit]) -> Result<()> {
    db.cache_title_search(query, hits)?;
    let dismissed = db.dismissed_suggestions(series_id)?;
    db.set_link_suggestion(series_id, query, best_match(query, hits, &dismissed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MediaTitle;
    use crate::manga::store::tests::{series, temp_db, volume};
    use crate::manga::store::SOURCE_LOCAL;

    fn hit(id: &str, romaji: &str, native: &str, english: Option<&str>) -> MediaHit {
        MediaHit {
            content_id: id.into(),
            title: MediaTitle { native: native.into(), romaji: Some(romaji.into()), english: english.map(Into::into) },
            content_image: Some(format!("https://img.example/{id}.jpg")),
            cover_image: None,
            is_adult: false,
            is_adult_image: None,
            synonyms: None,
            media_type: Some("manga".into()),
            episodes: None,
            episode_duration: None,
        }
    }

    #[test]
    fn titles_lose_release_tags_and_volume_numbers() {
        let cases = [
            ("20 Seiki Shounen (Upscaled)", "20 Seiki Shounen"),
            ("A Man and His Cat (HD Scan)", "A Man and His Cat"),
            ("[Oshi no Ko]", "Oshi no Ko"),
            ("#Zombie Sagashitemasu", "#Zombie Sagashitemasu"),
            ("Yotsuba&! Vol. 3", "Yotsuba&!"),
            ("Yotsuba&! v01-v15", "Yotsuba&!"),
            ("Berserk Volume 12 (Digital)", "Berserk"),
            ("[Group] Kingdom 01-45 [JP]", "Kingdom"),
            ("Shingeki_no_Kyojin", "Shingeki no Kyojin"),
            ("Mob Psycho 100", "Mob Psycho 100"),
            ("Hunter x Hunter - 03", "Hunter x Hunter"),
            ("よつばと！ 第3巻", "よつばと!"),
            ("【推しの子】", "推しの子"),
            ("Ｄｒ．ＳＴＯＮＥ", "Dr.STONE"),
            ("  Vinland Saga  ", "Vinland Saga"),
            ("Vol. 1", "Vol. 1"),
        ];
        for (raw, clean) in cases {
            assert_eq!(clean_title(raw), clean, "{raw}");
        }
    }

    #[test]
    fn similarity_reads_past_case_punctuation_and_width() {
        assert_eq!(similarity("Oshi no Ko", "OSHI NO KO"), 1.0);
        assert_eq!(similarity("Dr.STONE", "Dr. Stone"), 1.0);
        assert!(similarity("Yotsuba&!", "Yotsuba to!") > 0.7);
        assert!(similarity("Berserk", "Kingdom") < 0.2);
        assert_eq!(similarity("", "x"), 0.0);
    }

    #[test]
    fn the_best_title_of_the_best_hit_wins_above_the_threshold() {
        let hits = vec![
            hit("1", "Neko to Ojisan", "おじさまと猫", Some("A Man and His Cat")),
            hit("2", "Ojisama to Neko", "おじさまと猫", Some("A Man and His Cat")),
            hit("3", "Something Else", "別物", None),
        ];
        let (best, value) = best_match("A Man and His Cat", &hits, &HashSet::new()).unwrap();
        // Two equal scores: the search's own order breaks the tie.
        assert_eq!(best.content_id, "1");
        assert_eq!(value, 1.0);
        // Nothing alike: no suggestion at all.
        assert!(best_match("Vinland Saga", &hits, &HashSet::new()).is_none());
    }

    #[test]
    fn a_dismissed_media_is_never_the_suggestion() {
        let hits = vec![hit("1", "Oshi no Ko", "【推しの子】", None), hit("2", "Oshi no Ko: Ichigo", "推しの子 一護", None)];
        let dismissed: HashSet<String> = ["1".to_string()].into();
        let (best, _) = best_match("Oshi no Ko", &hits, &dismissed).unwrap();
        assert_eq!(best.content_id, "2");
        let both: HashSet<String> = ["1".to_string(), "2".to_string()].into();
        assert!(best_match("Oshi no Ko", &hits, &both).is_none());
    }

    #[test]
    fn the_pace_is_one_search_at_a_time_and_backs_off() {
        let mut pace = Pace::default();
        assert_eq!(pace.after(Outcome::Searched), Pace::INTERVAL);
        assert!(Pace::INTERVAL >= Duration::from_secs(1));
        assert_eq!(pace.after(Outcome::Cached), Duration::ZERO);
        assert_eq!(pace.after(Outcome::RateLimited), Duration::from_secs(30));
        assert_eq!(pace.after(Outcome::RateLimited), Duration::from_secs(60));
        assert_eq!(pace.after(Outcome::Failed), Duration::from_secs(120));
        for _ in 0..10 {
            pace.after(Outcome::Failed);
        }
        assert_eq!(pace.after(Outcome::RateLimited), Duration::from_secs(600));
        // An answer resets it.
        assert_eq!(pace.after(Outcome::Searched), Pace::INTERVAL);
        assert_eq!(pace.after(Outcome::Failed), Duration::from_secs(15));
        assert!(Pace::interruptible(Outcome::Idle));
        assert!(!Pace::interruptible(Outcome::RateLimited));
    }

    fn library() -> (Db, std::path::PathBuf) {
        let (db, path) = temp_db();
        db.add_manga_source("src", SOURCE_LOCAL, "Manga", "/manga").unwrap();
        db.apply_manga_scan(
            "src",
            SOURCE_LOCAL,
            &[
                series("s1", "A Man and His Cat (HD Scan)", vec![volume("v1", 1, 3)]),
                series("s2", "Berserk", vec![volume("v2", 1, 3)]),
                series("s3", "Akira", vec![volume("v3", 1, 3)]),
            ],
        )
        .unwrap();
        (db, path)
    }

    #[test]
    fn suggestions_never_link_and_follow_what_is_on_screen() {
        let (db, path) = library();
        db.link_manga_series("s3", Some("1"), Some("AKIRA"), false).unwrap();
        db.set_manga_progress("v2", 1, false).unwrap();
        // On screen first; then the last read; a linked series never.
        let Step::Search { series_id, query } = prepare(&db, &["s1".into()]).unwrap() else { panic!("a search") };
        assert_eq!((series_id.as_str(), query.as_str()), ("s1", "A Man and His Cat"));
        finish(&db, "s1", &query, &[hit("30", "Ojisama to Neko", "おじさまと猫", Some("A Man and His Cat"))]).unwrap();
        let Step::Search { series_id, .. } = prepare(&db, &[]).unwrap() else { panic!("a search") };
        assert_eq!(series_id, "s2");
        finish(&db, "s2", "Berserk", &[hit("9", "Kingdom", "キングダム", None)]).unwrap();
        assert!(matches!(prepare(&db, &[]).unwrap(), Step::Nothing));

        let waiting = db.link_suggestions().unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].media_id, "30");
        assert_eq!(waiting[0].media_title, "おじさまと猫");
        assert_eq!(waiting[0].media_subtitle.as_deref(), Some("Ojisama to Neko"));
        // A suggestion is only a suggestion.
        let s1 = db.manga_series("s1").unwrap().unwrap();
        assert_eq!(s1.media_id, None);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_title_is_searched_once_and_a_dismissal_sticks() {
        let (db, path) = library();
        let hits = [hit("30", "Ojisama to Neko", "おじさまと猫", Some("A Man and His Cat")), hit("31", "Ojisama to Neko 2", "おじさまと猫 2", Some("A Man and His Cat 2"))];
        finish(&db, "s1", "A Man and His Cat", &hits).unwrap();
        // Another folder of the same series: answered by the cache.
        db.apply_manga_scan("src", SOURCE_LOCAL, &[series("s4", "A Man and His Cat [Digital]", vec![volume("v4", 1, 3)])]).unwrap();
        assert!(matches!(prepare(&db, &["s4".into()]).unwrap(), Step::Done { .. }));
        assert_eq!(db.link_suggestion("s4").unwrap().unwrap().media_id, "30");

        db.apply_manga_scan(
            "src",
            SOURCE_LOCAL,
            &[series("s1", "A Man and His Cat (HD Scan)", vec![volume("v1", 1, 3)]), series("s4", "A Man and His Cat [Digital]", vec![volume("v4", 1, 3)])],
        )
        .unwrap();
        finish(&db, "s1", "A Man and His Cat", &hits).unwrap();
        assert!(db.dismiss_link_suggestion("s1").unwrap());
        // The next best of the same search, without the network.
        assert_eq!(db.link_suggestion("s1").unwrap().unwrap().media_id, "31");
        assert!(db.dismiss_link_suggestion("s1").unwrap());
        assert!(db.link_suggestion("s1").unwrap().is_none());
        // Searched again later, the dismissed ones stay out.
        finish(&db, "s1", "A Man and His Cat", &hits).unwrap();
        assert!(db.link_suggestion("s1").unwrap().is_none());
        // Other series are not affected.
        assert_eq!(db.link_suggestion("s4").unwrap().unwrap().media_id, "30");
        let _ = std::fs::remove_file(path);
    }
}
