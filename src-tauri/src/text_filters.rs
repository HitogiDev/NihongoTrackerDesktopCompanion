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

pub fn preview(filters: &NativeHookTextFilters, text: &str, reduce_repetitions: bool, filter_chinese: bool) -> FilterPreview {
    let normalized = normalize(text, reduce_repetitions, Some(filters))
        .map(|text| if filter_chinese { drop_chinese(&text) } else { text });
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
        let result = preview(&NativeHookTextFilters::default(), "【後輩】「ああ…………」", true, false);
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
        let result = preview(&filters, "太郎太郎[声][声]「こんにちはこんにちは」", true, false);
        assert_eq!(result.text, "「こんにちは」");
        assert_eq!(result.chars, 5);
        assert!(result.regex_error.is_none());
        let result = preview(&filters, "太郎太郎[声][声]「こんにちはこんにちは」", false, false);
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
        let result = preview(&filters, "名前：太郎[音声]「今日は雨だ。」[音声]", false, false);
        assert_eq!(result.text, "「今日は雨だ。」");
        assert_eq!(result.chars, 5);
        assert!(result.regex_error.is_none());
    }

    #[test]
    fn removed_lines_have_no_characters() {
        let filters = NativeHookTextFilters { excluded_words: "太郎".into(), ..Default::default() };
        let result = preview(&filters, "太郎", false, false);
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
        let first = preview(&filters, "太郎「今日は雨だ。今日は雨だ。」", true, false);
        let next = preview(&filters, "花子「今日は雨だ。今日は雨だ。」", true, false);
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
