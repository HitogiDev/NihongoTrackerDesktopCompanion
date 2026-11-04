use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NativeHookTextFilters {
    pub excluded_words: String,
    pub regex_patterns: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "FilterRegexError"))]
pub struct FilterError {
    pub line: usize,
    pub message: String,
}

pub struct CompiledFilters {
    patterns: Vec<Regex>,
    words: Vec<String>,
}

impl CompiledFilters {
    pub fn compile(settings: &NativeHookTextFilters) -> Result<Self, FilterError> {
        let mut patterns = Vec::new();
        for (index, pattern) in settings.regex_patterns.lines().enumerate() {
            if pattern.trim().is_empty() {
                continue;
            }
            patterns.push(Regex::new(pattern).map_err(|error| FilterError {
                line: index + 1,
                message: error.to_string(),
            })?);
        }
        let mut words: Vec<String> = settings.excluded_words.lines()
            .map(str::trim).filter(|word| !word.is_empty()).map(str::to_owned).collect();
        words.sort_by_key(|word| std::cmp::Reverse(word.len()));
        words.dedup();
        Ok(Self { patterns, words })
    }

    pub fn apply(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for pattern in &self.patterns {
            text = pattern.replace_all(&text, "").into_owned();
        }
        for word in &self.words {
            text = text.replace(word, "");
        }
        text.trim().to_owned()
    }
}

type FilterCache = Option<(NativeHookTextFilters, CompiledFilters)>;
static CACHE: Lazy<Mutex<FilterCache>> = Lazy::new(|| Mutex::new(None));
static REPETITIONS: Lazy<fancy_regex::Regex> = Lazy::new(||
    fancy_regex::Regex::new(r"(.{2,}?)\1").expect("The built-in repetition pattern is valid"));

fn replace_repeated_phrase(captures: &fancy_regex::Captures<'_, str>) -> String {
    let phrase = captures.get(1).unwrap().as_str();
    let mut letters = phrase.chars().filter(|character| character.is_alphanumeric());
    let first = letters.next();
    // Preserve single-character utterances and punctuation, including repeated ellipses.
    if letters.any(|character| Some(character) != first) {
        phrase.to_owned()
    } else {
        captures.get(0).unwrap().as_str().to_owned()
    }
}

pub fn apply(settings: &NativeHookTextFilters, text: &str) -> Result<String, FilterError> {
    let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    if cache.as_ref().is_none_or(|(previous, _)| previous != settings) {
        *cache = Some((settings.clone(), CompiledFilters::compile(settings)?));
    }
    Ok(cache.as_ref().unwrap().1.apply(text))
}

pub fn normalize(text: &str, reduce_repetitions: bool, filters: Option<&NativeHookTextFilters>) -> Result<String, FilterError> {
    let text = if reduce_repetitions {
        match REPETITIONS.try_replacen(text, 0, replace_repeated_phrase) {
            Ok(text) => text.into_owned(),
            Err(error) => {
                log::warn!("Could not reduce repeated text: {error}");
                text.to_owned()
            }
        }
    } else { text.to_owned() };
    match filters {
        Some(filters) => apply(filters, &text),
        None => Ok(text.trim().to_owned()),
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct FilterPreview {
    pub text: String,
    pub chars: i64,
    pub regex_error: Option<FilterError>,
}

/// Han characters that ordinary Japanese writing (almost) never uses — function
/// words and simplified/traditional forms of Chinese — plus Chinese-style
/// punctuation. One of them in a kana-free line is enough to call it Chinese.
///
/// Left out on purpose, because kana-free Japanese headings use them: 都 (東京都),
/// 過 (過去), 没 (没落), 着 (到着), 給 (給料), 把 (把握), 被 (被害), 還 (返還), 誰.
const CHINESE_MARKERS: &[char] = &[
    '的', '是', '这', '這', '们', '們', '说', '說', '么', '麼', '吗', '嗎', '呢', '吧',
    '啊', '你', '妳', '她', '沒', '还', '个', '对', '對', '让', '讓', '给', '过', '很',
    '哪', '谁', '什', '为', '吃', '跟', '呀', '嘛', '哦', '喔', '咱', '嗯', '哎', '唉', '喂',
    '呃', '远', '时', '们', '么', '这', '东', '见', '觉', '话', '样', '头', '问', '间',
    '，', '“', '”', '；',
];

/// Where a sentence or a line of dialogue ends: the seam between the original
/// and a translation glued after it.
const SENTENCE_END: &[char] = &['。', '！', '？', '!', '?', '」', '』', '）', ')', '】', '”', '…', '‥'];

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9D}')
}

fn is_han(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}')
}

/// Whether a line of hooked text is Chinese rather than Japanese.
///
/// Japanese prose always has kana, so a line with any kana is Japanese. Without
/// kana it can still be Japanese — a name, a place, 「了解」 — but those are
/// short, so a line is only called Chinese when it has at least four Han
/// characters and either a Chinese marker or the length of real dialogue.
pub fn is_chinese(line: &str) -> bool {
    if line.chars().any(is_kana) {
        return false;
    }
    let han = line.chars().filter(|c| is_han(*c)).count();
    if han < 4 {
        return false;
    }
    han >= 12 || line.chars().any(|c| CHINESE_MARKERS.contains(&c))
}

/// Whether a kana-free piece glued to Japanese on the same line is its Chinese
/// translation. `quoted` is the dialogue case — 「え…」“啊……” — where a “…”
/// right against a 「…」 is a giveaway: Japanese never pairs them that way.
/// Otherwise it needs a Chinese marker (punctuation counts) or real length, so
/// a trailing name or 了解 stays.
fn glued_chinese(part: &str, quoted: bool) -> bool {
    let han = part.chars().filter(|c| is_han(*c)).count();
    han > 0 && (quoted || han >= 10 || part.chars().any(|c| CHINESE_MARKERS.contains(&c) && c != '“' && c != '”'))
}

/// Cuts a Chinese translation off a line that also carries the Japanese
/// original. The Japanese part runs from the sentence holding the first kana to
/// the end of the sentence holding the last one; what is left on either side is
/// kana-free, and goes if it reads as Chinese.
fn drop_glued_chinese(line: &str) -> String {
    let Some(first) = line.find(is_kana) else { return line.to_owned() };
    let last = line.rfind(is_kana).unwrap_or(first);
    let after_last = last + line[last..].chars().next().map_or(0, char::len_utf8);
    // End of the sentence after the last kana, including the whole run of
    // closing marks (…」). No closing mark means nothing is glued after it.
    let end = match line[after_last..].find(SENTENCE_END) {
        Some(offset) => {
            let start = after_last + offset;
            line[start..].find(|c: char| !SENTENCE_END.contains(&c)).map_or(line.len(), |i| start + i)
        }
        None => line.len(),
    };
    let begin = line[..first].rfind(SENTENCE_END).map_or(0, |i| i + line[i..].chars().next().map_or(0, char::len_utf8));

    let japanese = &line[begin..end];
    let (before, after) = (&line[..begin], &line[end..]);
    let mut kept = String::new();
    let before_quoted = before.trim_end().ends_with('”') && japanese.starts_with(['「', '『']);
    if !glued_chinese(before, before_quoted) {
        kept.push_str(before);
    }
    kept.push_str(japanese);
    let after_quoted = after.trim_start().starts_with('“') && japanese.ends_with(['」', '』']);
    if !glued_chinese(after, after_quoted) {
        kept.push_str(after);
    }
    kept.trim().to_owned()
}

/// Drops the Chinese of a hook that sends a translation next to the original
/// (AIR and other dual-language releases). The translation can come on its own
/// line or glued to the Japanese one, so lines are judged one by one and a
/// mixed line keeps only its Japanese part.
pub fn drop_chinese(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !is_chinese(line))
        .map(drop_glued_chinese)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// The parts of hooked text that the game's own UI puts there rather than the
/// script: save-slot previews and load messages, and the speaker's name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameTextCleanup {
    pub drop_system_text: bool,
    pub strip_speaker_names: bool,
}

impl GameTextCleanup {
    pub fn from_settings(settings: &crate::config::Settings) -> Self {
        Self {
            drop_system_text: settings.texthooker_drop_system_text,
            strip_speaker_names: settings.texthooker_strip_speaker_names,
        }
    }
}

/// The date of a save slot (2026/02/17 13:28:24). A hook that also catches the
/// save or load screen sends every slot's last line followed by its date.
static SAVE_SLOT_DATE: Lazy<Regex> = Lazy::new(||
    Regex::new(r"\d{4}/\d{1,2}(?:/\d{1,2})?\s*\d{1,2}:\d{2}").expect("The save slot pattern is valid"));
/// 1番をロードしました, クイックセーブしました, ロードしますか？
static SAVE_LOAD_MESSAGE: Lazy<Regex> = Lazy::new(||
    Regex::new(r"^[^「」『』。]{0,12}?(?:クイック)?(?:セーブ|ロード)(?:しました|が完了しました|しますか|してよろしいですか)[。？?]?$")
        .expect("The save message pattern is valid"));

fn is_system_text(line: &str) -> bool {
    SAVE_SLOT_DATE.is_match(line) || SAVE_LOAD_MESSAGE.is_match(line)
}

/// Whether a piece of text reads as a speaker's name: short, and only kana,
/// kanji and the marks names use. No punctuation, quotes or digits, which keeps
/// out dialogue, narration and system messages.
fn is_speaker_name(text: &str) -> bool {
    let count = text.chars().count();
    (1..=8).contains(&count)
        && text.chars().any(|c| is_kana(c) || is_han(c))
        && text.chars().all(|c| is_kana(c) || is_han(c) || matches!(c, '々' | 'ー' | '・' | '＝') || c.is_alphabetic())
}

/// A name glued after a line of dialogue (「車で、轢くぞ」屈強な男). A particle
/// right after the quote is narration instead: 「うん」と言った.
fn is_trailing_name(text: &str) -> bool {
    is_speaker_name(text)
        && !text.starts_with(['と', 'っ', 'も', 'は', 'が', 'を', 'に', 'で', 'や', 'か', 'の', 'へ'])
}

/// Takes the speaker's name off a line of dialogue, whether the game puts it
/// before the quote (太郎「…」, 【太郎】「…」) or after it (「…」太郎). A line
/// that is only a name goes entirely.
fn strip_speaker_name(line: &str) -> &str {
    if is_speaker_name(line) {
        return "";
    }
    let mut line = line;
    if line.ends_with(['」', '』']) {
        if let Some(open) = line.find(['「', '『']) {
            let prefix = &line[..open];
            let bracketed = prefix.strip_prefix('【').and_then(|name| name.strip_suffix('】'));
            if bracketed.or(Some(prefix)).is_some_and(is_speaker_name) {
                line = &line[open..];
            }
        }
    }
    if line.starts_with(['「', '『']) {
        if let Some(close) = line.rfind(['」', '』']) {
            let end = close + line[close..].chars().next().map_or(0, char::len_utf8);
            if is_trailing_name(&line[end..]) {
                line = &line[..end];
            }
        }
    }
    line
}

/// Drops what the game's UI adds to a hook, line by line.
pub fn clean_game_text(text: &str, cleanup: GameTextCleanup) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !(cleanup.drop_system_text && is_system_text(line)))
        .map(|line| if cleanup.strip_speaker_names { strip_speaker_name(line) } else { line })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// How many lines back a repeat is caught, like Textractor's "Remove 30
/// Repeated Sentences".
const RECENT_LINES: usize = 30;
/// Short lines (「うん」) are said again for real, so only a quick repeat of one
/// is a redraw.
const SHORT_LINE_CHARS: i64 = 6;
const SHORT_LINE_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// The same line, or the same line with the speaker's name glued to one of them.
fn same_line(a: &str, b: &str) -> bool {
    let glued = |long: &str, short: &str| long.strip_prefix(short).is_some_and(is_speaker_name)
        || long.strip_suffix(short).is_some_and(is_speaker_name);
    a == b || glued(a, b) || glued(b, a)
}

/// The lines a work received last. Games redraw a line several times (once
/// per text box refresh, again with the name glued on), and each draw reaches
/// the hook.
#[derive(Default)]
pub struct RecentLines {
    by_content: std::collections::HashMap<String, std::collections::VecDeque<(String, std::time::Instant)>>,
}

impl RecentLines {
    /// Whether `text` repeats a recent line of `content_id`; it is remembered
    /// either way, so a line drawn over and over stays recent.
    pub fn is_repeat(&mut self, content_id: &str, text: &str, now: std::time::Instant) -> bool {
        let key: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let short = crate::texthooker::count_japanese(&key) < SHORT_LINE_CHARS;
        let recent = self.by_content.entry(content_id.to_owned()).or_default();
        let found = recent.iter().position(|(previous, at)| {
            same_line(previous, &key) && (!short || now.saturating_duration_since(*at) <= SHORT_LINE_WINDOW)
        });
        if let Some(index) = found {
            recent.remove(index);
        }
        recent.push_back((key, now));
        if recent.len() > RECENT_LINES {
            recent.pop_front();
        }
        found.is_some()
    }
}

static RECENT: Lazy<Mutex<RecentLines>> = Lazy::new(|| Mutex::new(RecentLines::default()));

pub fn is_recent_repeat(content_id: &str, text: &str) -> bool {
    RECENT.lock().unwrap_or_else(|error| error.into_inner())
        .is_repeat(content_id, text, std::time::Instant::now())
}

pub fn preview(filters: &NativeHookTextFilters, text: &str, reduce_repetitions: bool, filter_chinese: bool, cleanup: GameTextCleanup) -> FilterPreview {
    let normalized = normalize(text, reduce_repetitions, Some(filters))
        .map(|text| if filter_chinese { drop_chinese(&text) } else { text })
        .map(|text| clean_game_text(&text, cleanup));
    match normalized {
        Ok(text) => {
            let chars = crate::texthooker::count_japanese(&text);
            FilterPreview { text, chars, regex_error: None }
        }
        Err(error) => FilterPreview { text: String::new(), chars: 0, regex_error: Some(error) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_lines_are_told_apart_from_japanese_ones() {
        for chinese in [
            "我们走吧，天快黑了。",
            "這是我第一次來到這個小鎮。",
            "你在说什么？",
            "她没有回答我的问题",
            "夏天的海边总是让人想起很多事情",
        ] {
            assert!(is_chinese(chinese), "{chinese}");
        }
        for japanese in [
            "「おはよう、観鈴」",
            "神尾家居間",
            "了解！",
            "国崎往人",
            "……",
            "第一章　夏の日",
            "空にいる少女",
        ] {
            assert!(!is_chinese(japanese), "{japanese}");
        }
    }

    #[test]
    fn dual_language_text_keeps_only_the_japanese_lines() {
        assert_eq!(drop_chinese("「海に行こうよ」\n“我们去海边吧”"), "「海に行こうよ」");
        assert_eq!(drop_chinese("这是什么地方？"), "");
        assert_eq!(drop_chinese("神尾家居間"), "神尾家居間");
    }

    #[test]
    fn chinese_glued_to_the_japanese_line_is_cut_off() {
        for (input, expected) in [
            ("「え…」“啊……”", "「え…」"),
            ("みちるの呟きに、俺は遠野を見た。听到小满的呢喃声，我看向了远野。", "みちるの呟きに、俺は遠野を見た。"),
            ("どうしてだかはわからない。不知道为什么。", "どうしてだかはわからない。"),
            ("陽炎の向こうでゆれる遠野の姿が、とても哀しく見えた。远野的身影在热浪的另一边摇曳着，看上去十分悲伤。", "陽炎の向こうでゆれる遠野の姿が、とても哀しく見えた。"),
            ("瞬きをひとつするだけで消えてなくなりそうな、そんな儚さが少女をみたしていた。她的身上，充满了仿佛只要眨一下眼就会消失不见的虚幻感。", "瞬きをひとつするだけで消えてなくなりそうな、そんな儚さが少女をみたしていた。"),
            ("誰にも見えない。そこには誰もいないかのような、夏の空気。在夏日空气的笼罩下，谁也看不见她，仿佛那里不存在任何人。", "誰にも見えない。そこには誰もいないかのような、夏の空気。"),
            ("「…やっぱり…このままじゃ遠いよ…」“……果然……这样子太遥远了……”", "「…やっぱり…このままじゃ遠いよ…」"),
            ("「遠い…」“太遥远……”", "「遠い…」"),
            ("「うん」“嗯。”", "「うん」"),
            ("「ねえ、国崎往人。美凪をよんであげて」“哎，国崎往人。去叫一下美凪吧。”", "「ねえ、国崎往人。美凪をよんであげて」"),
            ("「ひとりは…きっとさびしいとおもうから…」“她一个人……一定会很寂寞的……”", "「ひとりは…きっとさびしいとおもうから…」"),
            ("「…そうだな。そうしよう」“……嗯，就这么办。”", "「…そうだな。そうしよう」"),
            ("“啊……”「え…」", "「え…」"),
        ] {
            assert_eq!(drop_chinese(input), expected, "{input}");
        }
        for japanese in [
            "そうか。了解。",
            "「了解」と言った。",
            "“魔王”と呼ばれた男。",
            "彼の名は“魔王”",
            "東京に行く日。国崎往人",
            "【後輩】「ああ……」",
        ] {
            assert_eq!(drop_chinese(japanese), japanese, "{japanese}");
        }
    }

    const ALL_CLEANUP: GameTextCleanup = GameTextCleanup { drop_system_text: true, strip_speaker_names: true };

    #[test]
    fn save_screen_and_load_messages_are_dropped() {
        for system in [
            "「まぁね。……ある程度切って、と」2026/05 13:36:47「まぁね。……ある程度切って、と」2026/05 13:36:44「大抵お目当てはモモ先輩とかの運動能力だけど2026/10/01 02:48:38",
            "「でないと、貴様の先輩のように」2026/02/17 13:28:24",
            "1番をロードしました",
            "クイックセーブしました。",
            "ロードしますか？",
        ] {
            assert_eq!(clean_game_text(system, ALL_CLEANUP), "", "{system}");
            assert_eq!(clean_game_text(system, GameTextCleanup::default()), system);
        }
        for script in ["「セーブしました、って言ったでしょ」", "ロードの途中で止まった。", "二時三十分。"] {
            assert_eq!(clean_game_text(script, ALL_CLEANUP), script, "{script}");
        }
    }

    #[test]
    fn speaker_names_are_taken_off_dialogue() {
        for (input, expected) in [
            ("「でないと、貴様の先輩のように」屈強な男", "「でないと、貴様の先輩のように」"),
            ("屈強な男", ""),
            ("一子", ""),
            ("太郎「今日は雨だ。」", "「今日は雨だ。」"),
            ("【後輩】「ああ……」", "「ああ……」"),
            ("ルーシー・ハート『はい』", "『はい』"),
            ("「うん」と言った", "「うん」と言った"),
            ("「うん」って頷いた。", "「うん」って頷いた。"),
            ("男は、強そうな獲物とは勝負するが、それ以外は効率良く事を処理するタイプだった。", "男は、強そうな獲物とは勝負するが、それ以外は効率良く事を処理するタイプだった。"),
            ("彼は「うん」と言った。", "彼は「うん」と言った。"),
        ] {
            assert_eq!(clean_game_text(input, ALL_CLEANUP), expected, "{input}");
        }
        assert_eq!(clean_game_text("「車で、轢くぞ」屈強な男", GameTextCleanup::default()), "「車で、轢くぞ」屈強な男");
    }

    #[test]
    fn redrawn_lines_are_skipped_even_with_the_name_glued_on() {
        let mut recent = RecentLines::default();
        let start = std::time::Instant::now();
        let mut kept = Vec::new();
        for (index, line) in [
            "「車で、轢くぞ」", "「車で、轢くぞ」", "「車で、轢くぞ」屈強な男",
            "男は、強そうな獲物とは勝負するが、それ以外は効率良く事を処理するタイプだった。",
            "男は、強そうな獲物とは勝負するが、それ以外は効率良く事を処理するタイプだった。",
            "一子", "「ふーっ、ここまで来れば九鬼クンも……」", "「ふーっ、ここまで来れば九鬼クンも……」",
            "「ふーっ、ここまで来れば九鬼クンも……」一子", "「ふーっ、ここまで来れば九鬼クンも……」",
            "英雄", "「一子殿どこへ行った？まだ我の話は終わっていないというのに」",
        ].into_iter().enumerate() {
            if !recent.is_repeat("v1", line, start + std::time::Duration::from_millis(index as u64 * 300)) {
                kept.push(line);
            }
        }
        assert_eq!(kept, [
            "「車で、轢くぞ」",
            "男は、強そうな獲物とは勝負するが、それ以外は効率良く事を処理するタイプだった。",
            "一子", "「ふーっ、ここまで来れば九鬼クンも……」",
            "英雄", "「一子殿どこへ行った？まだ我の話は終わっていないというのに」",
        ]);
    }

    #[test]
    fn short_lines_only_repeat_within_a_few_seconds() {
        let mut recent = RecentLines::default();
        let start = std::time::Instant::now();
        assert!(!recent.is_repeat("v1", "「うん」", start));
        assert!(recent.is_repeat("v1", "「うん」", start + std::time::Duration::from_secs(1)));
        assert!(!recent.is_repeat("v1", "「うん」", start + std::time::Duration::from_secs(30)));
        assert!(!recent.is_repeat("v2", "「うん」", start + std::time::Duration::from_secs(30)));
        let long = "「今日はいい天気だから散歩に行こう」";
        assert!(!recent.is_repeat("v1", long, start));
        assert!(recent.is_repeat("v1", long, start + std::time::Duration::from_secs(600)));
    }

    #[test]
    fn empty_filters_preserve_dialogue() {
        assert_eq!(apply(&NativeHookTextFilters::default(), " 太郎「こんにちは」 ").unwrap(), "太郎「こんにちは」");
    }

    #[test]
    fn repetition_filter_removes_phrases_and_preserves_expressive_characters() {
        for (input, expected) in [
            ("こんにちはこんにちは", "こんにちは"),
            ("ああいいうう", "ああいいうう"),
            ("日本語日本語、本文本文！", "日本語、本文！"),
            ("aaaa", "aaaa"),
            ("abababa", "ababa"),
            ("𠮷𠮷😀😀", "𠮷𠮷😀😀"),
            ("【後輩】「ああ……」", "【後輩】「ああ……」"),
            ("【後輩】「ああ…………」", "【後輩】「ああ…………」"),
            ("ああああ……！？！？", "ああああ……！？！？"),
            ("あ…あ…", "あ…あ…"),
            ("「はい、はい……」「はい、はい……」", "「はい、はい……」"),
            ("𠮷野𠮷野", "𠮷野"),
            ("本文\n本文", "本文\n本文"),
            ("今日は雨です。", "今日は雨です。"),
        ] {
            assert_eq!(normalize(input, true, None).unwrap(), expected, "{input}");
            assert_eq!(normalize(input, false, None).unwrap(), input);
        }
    }

    #[test]
    fn expressive_dialogue_keeps_its_characters_in_preview_and_storage() {
        let result = preview(&NativeHookTextFilters::default(), "【後輩】「ああ…………」", true, false, GameTextCleanup::default());
        assert_eq!(result.text, "【後輩】「ああ…………」");
        assert_eq!(result.chars, 4);
        let db = crate::db::Db::open(std::path::Path::new(":memory:")).unwrap();
        db.insert_line(&crate::db::TextLine {
            id: "expressive".into(), content_id: "v1".into(), text: result.text,
            chars: result.chars, created_at: crate::db::now_iso(), synced: false,
        }).unwrap();
        assert_eq!(db.text_totals("v1"), (1, 4));
    }

    #[test]
    fn repetition_filter_runs_before_removal_rules_and_character_counting() {
        let filters = NativeHookTextFilters {
            regex_patterns: r"\[[^\]]*\]".into(), excluded_words: "太郎".into(),
        };
        let result = preview(&filters, "太郎太郎[声][声]「こんにちはこんにちは」", true, false, GameTextCleanup::default());
        assert_eq!(result.text, "「こんにちは」");
        assert_eq!(result.chars, 5);
        assert!(result.regex_error.is_none());
        let result = preview(&filters, "太郎太郎[声][声]「こんにちはこんにちは」", false, false, GameTextCleanup::default());
        assert_eq!(result.text, "「こんにちはこんにちは」");
        assert_eq!(result.chars, 10);
    }

    #[test]
    fn names_are_literal_and_longest_first() {
        let filters = NativeHookTextFilters { excluded_words: "太郎\n太郎君\n[声]\n".into(), ..Default::default() };
        assert_eq!(apply(&filters, "太郎君[声]「太郎、こんにちは」").unwrap(), "「、こんにちは」");
    }

    #[test]
    fn patterns_are_applied_in_order_before_names_and_counting() {
        let filters = NativeHookTextFilters {
            regex_patterns: "^名前：\n\\[[^\\]]*\\]".into(),
            excluded_words: "太郎".into(),
        };
        let result = preview(&filters, "名前：太郎[音声]「今日は雨だ。」[音声]", false, false, GameTextCleanup::default());
        assert_eq!(result.text, "「今日は雨だ。」");
        assert_eq!(result.chars, 5);
        assert!(result.regex_error.is_none());
    }

    #[test]
    fn removed_lines_have_no_characters() {
        let filters = NativeHookTextFilters { excluded_words: "太郎".into(), ..Default::default() };
        let result = preview(&filters, "太郎", false, false, GameTextCleanup::default());
        assert!(result.text.is_empty());
        assert_eq!(result.chars, 0);
    }

    #[test]
    fn invalid_regex_reports_original_line_and_does_not_replace_cache() {
        let valid = NativeHookTextFilters::default();
        assert_eq!(apply(&valid, "本文").unwrap(), "本文");
        let invalid = NativeHookTextFilters { regex_patterns: "\n^名前\n[".into(), ..Default::default() };
        assert_eq!(apply(&invalid, "本文").unwrap_err().line, 3);
        assert_eq!(apply(&valid, "本文").unwrap(), "本文");
    }

    #[test]
    fn settings_from_previous_versions_have_empty_filters() {
        let settings: crate::config::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.native_hook_text_filters, NativeHookTextFilters::default());
    }

    #[test]
    fn repeated_dialogue_is_stored_as_separate_lines_with_cleaned_character_counts() {
        let db = crate::db::Db::open(std::path::Path::new(":memory:")).unwrap();
        let filters = NativeHookTextFilters { excluded_words: "太郎\n花子".into(), ..Default::default() };
        let first = preview(&filters, "太郎「今日は雨だ。今日は雨だ。」", true, false, GameTextCleanup::default());
        let next = preview(&filters, "花子「今日は雨だ。今日は雨だ。」", true, false, GameTextCleanup::default());
        assert_eq!(first.text, next.text);
        db.insert_line(&crate::db::TextLine {
            id: "test-filtered".into(), content_id: "v1".into(),
            text: first.text, chars: first.chars, created_at: crate::db::now_iso(), synced: false,
        }).unwrap();
        assert!(db.insert_line(&crate::db::TextLine {
            id: "test-filtered-next".into(), content_id: "v1".into(),
            text: next.text, chars: next.chars, created_at: crate::db::now_iso(), synced: false,
        }).unwrap());
        assert_eq!(db.text_totals("v1"), (2, 10));
    }
}
