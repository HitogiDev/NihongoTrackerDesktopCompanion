//! Anime release-name parser — a compact, dependency-free take on Anitomy.
//!
//! Good enough for the shapes fansub / streaming rips actually use:
//!   `[Group] Title - 05v2 [1080p][HEVC].mkv`
//!   `Title.S02E07.1080p.WEB-DL.x265.mkv`
//!   `[Group] Title 第08話 (BD 1920x1080).mkv`
//!   `Title - 05-06 (Batch).mkv`

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ParsedAnime {
    pub title: String,
    pub episode: Option<u32>,
    /// Set when the file covers a range, e.g. `05-06`.
    pub episode_end: Option<u32>,
    pub season: Option<u32>,
    pub group: Option<String>,
    pub file_name: String,
}

impl ParsedAnime {
    /// Number of episodes this file accounts for.
    pub fn episode_count(&self) -> u32 {
        match (self.episode, self.episode_end) {
            (Some(a), Some(b)) if b >= a => b - a + 1,
            (Some(_), _) => 1,
            _ => 1,
        }
    }
}

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "ogm", "rm", "rmvb", "wmv", "mpg", "mpeg", "m4v", "mov", "flv", "webm",
    "ts", "m2ts", "divx", "asf", "vob",
];

/// Tokens that are never part of a title and can be trimmed off its tail.
const NOISE: &[&str] = &[
    "1080p", "720p", "480p", "360p", "540p", "1440p", "2160p", "4k", "8k", "hd", "fhd", "uhd", "sd",
    "x264", "x265", "h264", "h265", "avc", "hevc", "10bit", "8bit", "10bits", "8bits", "hi10p",
    "hi10", "aac", "flac", "opus", "ac3", "eac3", "dts", "mp3", "vorbis", "2.0", "5.1", "bd",
    "bdrip", "bluray", "blu-ray", "brrip", "web", "webrip", "web-dl", "webdl", "hdtv", "dvd",
    "dvdrip", "dvd-rip", "tvrip", "raw", "remux", "repack", "batch", "uncensored", "censored",
    "multi", "dual", "dualaudio", "subs", "sub", "subbed", "dub", "dubbed", "eng", "esp", "spa",
    "jpn", "jap", "vostfr", "final", "end", "complete", "nc", "nced", "ncop", "sfx", "yuv420p8",
    "aac2", "opus2", "ma", "amzn", "crunchyroll", "cr", "funi", "netflix", "nf", "hulu", "disney",
];

/// Type markers that are worth keeping in the title (they disambiguate entries).
const KEEP_MARKERS: &[&str] = &["ova", "ona", "movie", "special", "sp", "oad"];

static RE_BRACKETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\[\(\{][^\[\]\(\)\{\}]*[\]\)\}]").unwrap());
static RE_LEADING_GROUP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*\[([^\]]+)\]").unwrap());
static RE_SXXEXX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bs(\d{1,2})[ ._-]?e(\d{1,4})\b").unwrap());
static RE_JP_EPISODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"第\s*(\d{1,4})\s*話").unwrap());
static RE_EP_KEYWORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?:episode|episodio|ep|e)[ ._-]?(\d{1,4})(?:v\d+)?\b").unwrap());
static RE_DASH_RANGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)-\s*(\d{1,4})\s*[-~]\s*(\d{1,4})(?:\s|$)").unwrap());
static RE_DASH_EPISODE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)-\s*(\d{1,4})(?:v\d+)?(?:\s|$)").unwrap());
static RE_BARE_NUMBER: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,4})(?:v\d+)?\b").unwrap());
static RE_SEASON_WORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:season\s*(\d{1,2})|(\d{1,2})(?:nd|rd|th|st)\s*season|s(\d{1,2}))\b")
        .unwrap()
});
static RE_SEASON_JP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d{1,2})\s*期").unwrap());
static RE_WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());

fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) => stem,
        _ => name,
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn is_noise(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
    if t.is_empty() {
        return true;
    }
    if KEEP_MARKERS.contains(&t.as_str()) {
        return false;
    }
    if NOISE.contains(&t.as_str()) {
        return true;
    }
    // 1920x1080 style, or a bare CRC32.
    if t.len() == 8 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    Regex::new(r"^\d{3,4}x\d{3,4}$").unwrap().is_match(&t)
}

/// Numbers that are obviously not episode numbers.
fn plausible_episode(n: u32, digits: usize) -> bool {
    if n == 0 && digits <= 1 {
        return false;
    }
    if digits == 4 && (1900..=2100).contains(&n) {
        return false;
    }
    !matches!(n, 264 | 265 | 360 | 480 | 540 | 720 | 1080 | 1440 | 2160)
}

fn tidy(text: &str) -> String {
    let text = text.replace(['_'], " ");
    let text = RE_WS.replace_all(&text, " ");
    text.trim()
        .trim_end_matches(|c: char| c == '-' || c == '–' || c == '—' || c == ':' || c == '.')
        .trim()
        .to_string()
}

/// Drop trailing release-metadata tokens from a title.
fn trim_noise_tail(title: &str) -> String {
    let mut tokens: Vec<&str> = title.split_whitespace().collect();
    while let Some(last) = tokens.last() {
        if is_noise(last) && tokens.len() > 1 {
            tokens.pop();
        } else {
            break;
        }
    }
    tokens.join(" ")
}

fn extract_season(title: &str) -> (String, Option<u32>) {
    if let Some(c) = RE_SEASON_JP.captures(title) {
        let season = c.get(1).and_then(|m| m.as_str().parse().ok());
        let stripped = title.replacen(c.get(0).unwrap().as_str(), "", 1);
        return (tidy(&stripped), season);
    }
    if let Some(c) = RE_SEASON_WORD.captures(title) {
        let season = (1..=3)
            .filter_map(|i| c.get(i))
            .find_map(|m| m.as_str().parse::<u32>().ok());
        // Only strip when it sits at the tail; "Season" inside a real title is rare
        // but "S2" mid-title would otherwise mangle the name.
        let whole = c.get(0).unwrap();
        let stripped = if whole.end() >= title.trim_end().len() {
            title[..whole.start()].to_string()
        } else {
            title.to_string()
        };
        return (tidy(&stripped), season);
    }
    (title.to_string(), None)
}

/// Parse a file name (or a player window title that contains one).
pub fn parse(input: &str) -> ParsedAnime {
    let file_name = base_name(input).trim().to_string();
    let stem = strip_extension(&file_name).to_string();

    let group = RE_LEADING_GROUP
        .captures(&stem)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|g| !g.is_empty() && g.len() <= 40 && !g.chars().all(|c| c.is_ascii_digit()));

    // Keep bracket contents out of episode detection, but remember where the
    // core text started so titles that are entirely bracketed still work.
    let mut core = RE_BRACKETS.replace_all(&stem, " ").to_string();
    if !core.contains(' ') && core.matches('.').count() >= 2 {
        core = core.replace('.', " ");
    }
    core = tidy(&core);
    if core.is_empty() {
        core = tidy(&stem.replace(['[', ']', '(', ')'], " "));
    }

    let mut result = ParsedAnime {
        file_name: file_name.clone(),
        group,
        ..Default::default()
    };

    // 1. SxxExx
    if let Some(c) = RE_SXXEXX.captures(&core) {
        result.season = c.get(1).and_then(|m| m.as_str().parse().ok());
        result.episode = c.get(2).and_then(|m| m.as_str().parse().ok());
        result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
        if result.title.is_empty() {
            result.title = trim_noise_tail(&tidy(&core));
        }
        return finish(result);
    }

    // 2. 第NN話
    if let Some(c) = RE_JP_EPISODE.captures(&core) {
        result.episode = c.get(1).and_then(|m| m.as_str().parse().ok());
        result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
        return finish(result);
    }

    // 3. "- 05-06" range
    if let Some(c) = RE_DASH_RANGE.captures(&core) {
        let a: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        let b: u32 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        if plausible_episode(a, c.get(1).unwrap().as_str().len()) && b >= a {
            result.episode = Some(a);
            result.episode_end = Some(b);
            result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
            return finish(result);
        }
    }

    // 4. "Episode 05" / "EP05" / "E05"
    if let Some(c) = RE_EP_KEYWORD.captures(&core) {
        let n: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        if plausible_episode(n, c.get(1).unwrap().as_str().len()) {
            result.episode = Some(n);
            result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
            return finish(result);
        }
    }

    // 5. "Title - 05" (last dash-number wins; titles often contain " - ")
    if let Some(c) = RE_DASH_EPISODE.captures_iter(&core).last() {
        let n: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        if plausible_episode(n, c.get(1).unwrap().as_str().len()) {
            result.episode = Some(n);
            result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
            return finish(result);
        }
    }

    // 6. Last plausible bare number.
    let candidate = RE_BARE_NUMBER
        .captures_iter(&core)
        .filter(|c| {
            let m = c.get(1).unwrap();
            m.as_str()
                .parse::<u32>()
                .map(|n| plausible_episode(n, m.as_str().len()))
                .unwrap_or(false)
        })
        .last();
    if let Some(c) = candidate {
        result.episode = c.get(1).and_then(|m| m.as_str().parse().ok());
        result.title = trim_noise_tail(&tidy(&core[..c.get(0).unwrap().start()]));
        if result.title.is_empty() {
            result.title = trim_noise_tail(&tidy(&core));
        }
        return finish(result);
    }

    // No episode at all — probably a movie.
    result.title = trim_noise_tail(&tidy(&core));
    finish(result)
}

fn finish(mut result: ParsedAnime) -> ParsedAnime {
    if result.season.is_none() {
        let (title, season) = extract_season(&result.title);
        result.title = title;
        result.season = season;
    }
    result.title = tidy(&result.title);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fansub_dash_form() {
        let p = parse("[SubsPlease] Sousou no Frieren - 05 (1080p) [A1B2C3D4].mkv");
        assert_eq!(p.title, "Sousou no Frieren");
        assert_eq!(p.episode, Some(5));
        assert_eq!(p.group.as_deref(), Some("SubsPlease"));
    }

    #[test]
    fn scene_dot_form() {
        let p = parse("Vinland.Saga.S02E07.1080p.WEB-DL.x265.mkv");
        assert_eq!(p.title, "Vinland Saga");
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episode, Some(7));
    }

    #[test]
    fn japanese_counter_form() {
        let p = parse("[Group] 葬送のフリーレン 第08話 (BD 1920x1080).mkv");
        assert_eq!(p.title, "葬送のフリーレン");
        assert_eq!(p.episode, Some(8));
    }

    #[test]
    fn version_suffix_and_range() {
        assert_eq!(parse("[G] Show - 12v2 [720p].mkv").episode, Some(12));
        let batch = parse("Show - 05-06 (Batch).mkv");
        assert_eq!(batch.episode, Some(5));
        assert_eq!(batch.episode_end, Some(6));
        assert_eq!(batch.episode_count(), 2);
    }

    #[test]
    fn title_containing_dashes() {
        let p = parse("[G] Fate - Stay Night - 03 [1080p].mkv");
        assert_eq!(p.title, "Fate - Stay Night");
        assert_eq!(p.episode, Some(3));
    }

    #[test]
    fn ignores_resolution_and_year() {
        let p = parse("Bakemonogatari (2009) - 01 [1080p].mkv");
        assert_eq!(p.episode, Some(1));
    }

    #[test]
    fn season_word_form() {
        let p = parse("[G] Spy x Family Season 2 - 04 [1080p].mkv");
        assert_eq!(p.title, "Spy x Family");
        assert_eq!(p.season, Some(2));
        assert_eq!(p.episode, Some(4));
    }
}
