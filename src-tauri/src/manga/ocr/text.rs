//! From the recognizer's token ids to the text manga-ocr would give.
//!
//! manga-ocr (kha-white/manga-ocr, Apache-2.0) decodes with its BERT Japanese
//! character tokenizer, skipping special tokens, and then `post_process`es:
//!
//! ```python
//! text = "".join(text.split())
//! text = text.replace("…", "...")
//! text = re.sub("[・.]{2,}", lambda x: (x.end() - x.start()) * ".", text)
//! text = jaconv.h2z(text, ascii=True, digit=True)
//! ```
//!
//! `h2z` turns half-width ASCII, digits and katakana into their full-width
//! forms (`...` becomes `．．．`). Reproduced here so the text matches what the
//! same model gives in mokuro and owocr.

/// `[PAD]`, `[UNK]`, `[CLS]`, `[SEP]` and `[MASK]`: the first five entries
/// of manga-ocr's `vocab.txt`, all skipped when decoding.
const SPECIAL_TOKENS: i64 = 5;
/// `decoder_start_token_id` (`[CLS]`).
pub const BOS: i64 = 2;
/// `eos_token_id` (`[SEP]`).
pub const EOS: i64 = 3;

/// One token per line, as `vocab.txt` has them.
pub fn parse_vocab(text: &str) -> Vec<String> {
    text.lines().map(|line| line.trim_end_matches('\r').to_string()).collect()
}

/// The tokens' text, as `tokenizer.decode(ids, skip_special_tokens=True)`
/// followed by manga-ocr's `post_process`.
pub fn decode(ids: &[i64], vocab: &[String]) -> String {
    let mut text = String::new();
    for &id in ids {
        if id < SPECIAL_TOKENS {
            continue;
        }
        let Some(token) = usize::try_from(id).ok().and_then(|id| vocab.get(id)) else { continue };
        // A word piece's continuation marker; the tokens are characters, but
        // the vocabulary is BERT's.
        text.push_str(token.strip_prefix("##").unwrap_or(token));
    }
    post_process(&text)
}

pub fn post_process(text: &str) -> String {
    let joined: String = text.split_whitespace().collect();
    let dotted = joined.replace('…', "...");
    // Runs of two or more `・` or `.` become as many dots.
    let mut normalized = String::with_capacity(dotted.len());
    let chars: Vec<char> = dotted.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '・' || chars[i] == '.' {
            let start = i;
            while i < chars.len() && (chars[i] == '・' || chars[i] == '.') {
                i += 1;
            }
            if i - start >= 2 {
                normalized.extend(std::iter::repeat('.').take(i - start));
            } else {
                normalized.push(chars[start]);
            }
        } else {
            normalized.push(chars[i]);
            i += 1;
        }
    }
    to_full_width(&normalized)
}

/// jaconv's half-width katakana with a voicing mark, as one full-width kana.
const VOICED: &[(&str, char)] = &[
    ("ｶﾞ", 'ガ'), ("ｷﾞ", 'ギ'), ("ｸﾞ", 'グ'), ("ｹﾞ", 'ゲ'), ("ｺﾞ", 'ゴ'),
    ("ｻﾞ", 'ザ'), ("ｼﾞ", 'ジ'), ("ｽﾞ", 'ズ'), ("ｾﾞ", 'ゼ'), ("ｿﾞ", 'ゾ'),
    ("ﾀﾞ", 'ダ'), ("ﾁﾞ", 'ヂ'), ("ﾂﾞ", 'ヅ'), ("ﾃﾞ", 'デ'), ("ﾄﾞ", 'ド'),
    ("ﾊﾞ", 'バ'), ("ﾋﾞ", 'ビ'), ("ﾌﾞ", 'ブ'), ("ﾍﾞ", 'ベ'), ("ﾎﾞ", 'ボ'),
    ("ﾊﾟ", 'パ'), ("ﾋﾟ", 'ピ'), ("ﾌﾟ", 'プ'), ("ﾍﾟ", 'ペ'), ("ﾎﾟ", 'ポ'),
    ("ｳﾞ", 'ヴ'),
];
/// The rest of jaconv's half-width kana table, character for character.
const HALF_KANA: &str = "ｧｱｨｲｩｳｪｴｫｵｶｷｸｹｺｻｼｽｾｿﾀﾁｯﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓｬﾔｭﾕｮﾖﾗﾘﾙﾚﾛﾜｦﾝｰ･｢｣｡､";
const FULL_KANA: &str = "ァアィイゥウェエォオカキクケコサシスセソタチッツテトナニヌネノハヒフヘホマミムメモャヤュユョヨラリルレロワヲンー・「」。、";

/// `jaconv.h2z(text, ascii=True, digit=True)`: ASCII (`!`…`~`) moves to its
/// full-width block, a space becomes an ideographic space, and half-width
/// katakana become full-width (voiced ones first, as one character).
pub fn to_full_width(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    let mut rest = text;
    'outer: while let Some(c) = rest.chars().next() {
        for (pair, full) in VOICED {
            if let Some(after) = rest.strip_prefix(pair) {
                out.push(*full);
                rest = after;
                continue 'outer;
            }
        }
        let mapped = match c {
            '!'..='~' => char::from_u32(c as u32 - 0x21 + 0xFF01).unwrap_or(c),
            ' ' => '\u{3000}',
            _ => HALF_KANA.chars().position(|half| half == c).and_then(|index| FULL_KANA.chars().nth(index)).unwrap_or(c),
        };
        out.push(mapped);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vec<String> {
        parse_vocab("[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nあ\nい\n.\n…\n##う\n1\nA\n")
    }

    #[test]
    fn special_tokens_are_skipped_and_pieces_joined() {
        assert_eq!(decode(&[BOS, 5, 1, 6, 9, EOS, 0], &vocab()), "あいう");
        // Out of range ids are not a panic.
        assert_eq!(decode(&[5, 9999, -1], &vocab()), "あ");
    }

    #[test]
    fn post_processing_matches_manga_ocr() {
        assert_eq!(decode(&[5, 8], &vocab()), "あ．．．");
        assert_eq!(post_process("え ・・・ ほんと?"), "え．．．ほんと？");
        assert_eq!(post_process("a・b"), "ａ・ｂ");
        assert_eq!(post_process("10円"), "１０円");
        assert_eq!(post_process("ｶﾞﾝﾊﾞﾚ!"), "ガンバレ！");
        assert_eq!(post_process("ｳﾞｧｲｵﾘﾝ｡"), "ヴァイオリン。");
        assert_eq!(post_process("全角のまま。"), "全角のまま。");
        assert_eq!(to_full_width("~ "), "～\u{3000}");
    }

    #[test]
    fn the_kana_tables_line_up() {
        assert_eq!(HALF_KANA.chars().count(), FULL_KANA.chars().count());
    }
}
