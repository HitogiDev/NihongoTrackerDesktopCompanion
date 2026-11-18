//! meikiocr as a built-in engine: the "fast" one. Two small D-FINE models
//! by rtr46 (LGPL-3.0, downloaded on demand like manga-ocr's, see `assets`),
//! run by the same ONNX Runtime. The pipeline is reimplemented here from
//! what meikiocr 0.3.5 (rtr46/meikiocr, Apache-2.0, `meikiocr/ocr.py`) does;
//! none of its code is in the app.
//!
//! 1. **Detect lines** (`meiki.text.detect.v0.1.960x544`): the image scaled
//!    to fit 960×544 keeping its shape, at the top left of a black canvas, as
//!    BGR in `[0, 1]` (meikiocr hands OpenCV's BGR straight to the models),
//!    plus `[960 / scale, 544 / scale]` as the target size, so the boxes come
//!    back in image pixels. Lines scoring above 0.5 are kept; the model gives
//!    64 at most. Unlike meikiocr, a page taller than that shape is detected
//!    in overlapping bands (`bands`, `merge_lines`): whole, a manga page
//!    would shrink to a third and lose its small text.
//! 2. **Read each line**, as a detector of characters: a line wider than it
//!    is tall goes to the horizontal model (scaled to 32 px high, at most
//!    960 wide, padded right and below); a taller one to the vertical model
//!    (32 px wide, up to 480 high; a longer column is cut into 420 px pieces
//!    that overlap by 64). Each gives up to 48 (24) characters as Unicode
//!    code points with a box and a score; those above 0.1 are mapped back to
//!    the page, and along the line the most confident wins wherever two
//!    overlap by more than 30 %. What is left, in order along the line, is
//!    the text.
//! 3. **Lay out**: meikiocr stops at lines. A manga page wants blocks (a
//!    bubble, with its columns), so furigana is dropped (`is_furigana`),
//!    neighbouring lines written the same way and of a similar size are
//!    joined (`blocks`), and the blocks go in the reading order of the other
//!    engines (`layout::reading_order`).
//!
//! Its models were trained on video games: on manga it is much faster than
//! manga-ocr but reads less well, and its vertical model is a beta.

use std::path::{Path, PathBuf};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ort::session::Session;
use ort::value::Tensor;

use super::assets;
use super::engine::{environment, session, Provider};
use crate::manga::mokuro::{MokuroBlock, MokuroPage};

const DET_WIDTH: u32 = 960;
const DET_HEIGHT: u32 = 544;
const REC_WIDTH: u32 = 960;
const REC_HEIGHT: u32 = 32;
const VREC_WIDTH: u32 = 32;
const VREC_HEIGHT: u32 = 480;
/// A column longer than the model is read in pieces this high…
const VREC_SEGMENT: f64 = 420.0;
/// …that overlap this much (both in the model's pixels).
const VREC_OVERLAP: f64 = 64.0;
pub const DET_THRESHOLD: f32 = 0.5;
pub const REC_THRESHOLD: f32 = 0.1;
/// Two characters overlapping more than this along the line are one.
const SAME_CHARACTER: f64 = 0.3;
/// Lines read per run of a recognizer.
const BATCH: usize = 8;

/// Kanji pairs the recognizer tends to give the wrong way round (meikiocr's
/// own list).
const SWAPPED_PAIRS: &[(&str, &str)] = &[
    ("儡傀", "傀儡"),
    ("談冗", "冗談"),
    ("汰淘", "淘汰"),
    ("沱滂", "滂沱"),
    ("攣痙", "痙攣"),
    ("酊酩", "酩酊"),
    ("麭麺", "麺麭"),
    ("哭慟", "慟哭"),
];

/// BGR, channels first, in `[0, 1]`, into `out` at `offset` of a tensor
/// whose planes are `plane` values and rows `stride` values wide.
fn put_bgr(image: &DynamicImage, out: &mut [f32], stride: usize, plane: usize) {
    let rgb = image.to_rgb8();
    for (x, y, pixel) in rgb.enumerate_pixels() {
        let i = y as usize * stride + x as usize;
        out[i] = f32::from(pixel[2]) / 255.0;
        out[plane + i] = f32::from(pixel[1]) / 255.0;
        out[2 * plane + i] = f32::from(pixel[0]) / 255.0;
    }
}

/// The detector's input for a page: the page fitted into 960×544 at the top
/// left, and the target size that makes its boxes come out in page pixels.
pub fn detector_input(image: &DynamicImage) -> (Vec<f32>, [i64; 2]) {
    let (w, h) = image.dimensions();
    let scale = (f64::from(DET_WIDTH) / f64::from(w)).min(f64::from(DET_HEIGHT) / f64::from(h));
    let rw = ((f64::from(w) * scale) as u32).clamp(1, DET_WIDTH);
    let rh = ((f64::from(h) * scale) as u32).clamp(1, DET_HEIGHT);
    let plane = (DET_WIDTH * DET_HEIGHT) as usize;
    let mut tensor = vec![0f32; plane * 3];
    put_bgr(&image.resize_exact(rw, rh, FilterType::Triangle), &mut tensor, DET_WIDTH as usize, plane);
    let sizes = [(f64::from(DET_WIDTH) / scale) as i64, (f64::from(DET_HEIGHT) / scale) as i64];
    (tensor, sizes)
}

/// The confident lines, inside the page, in whole pixels, top first.
pub fn line_boxes(boxes: &[f32], scores: &[f32], image: (u32, u32), threshold: f32) -> Vec<[u32; 4]> {
    let mut out: Vec<[u32; 4]> = boxes
        .chunks_exact(4)
        .zip(scores)
        .filter(|(b, &score)| score > threshold && b.iter().all(|v| v.is_finite()))
        .map(|(b, _)| {
            let clamp = |v: f32, max: u32| (v.max(0.0) as u32).min(max);
            [clamp(b[0], image.0), clamp(b[1], image.1), clamp(b[2], image.0), clamp(b[3], image.1)]
        })
        .filter(|b| b[2] > b[0] && b[3] > b[1])
        .collect();
    out.sort_by_key(|b| b[1]);
    out
}

/// How much of a band the next one covers again.
const BAND_OVERLAP: f64 = 0.25;

/// The bands a page is detected in, `(top, bottom)`. meikiocr fits the whole
/// image into 960×544, made for a game's screen: a manga page, taller than
/// wide, would shrink to a third of its size (small text, furigana and
/// captions are lost) and the detector stops at 64 lines. So a page taller
/// than that shape is cut into bands of that shape, a quarter of each
/// overlapping the next; a wide image is one band.
pub fn bands(width: u32, height: u32) -> Vec<(u32, u32)> {
    let band = (f64::from(width) * f64::from(DET_HEIGHT) / f64::from(DET_WIDTH)).round().max(1.0);
    if f64::from(height) <= band * 1.05 {
        return vec![(0, height)];
    }
    let stride = band * (1.0 - BAND_OVERLAP);
    let count = ((f64::from(height) - band) / stride).ceil() as u32 + 1;
    let stride = (f64::from(height) - band) / f64::from(count - 1);
    (0..count)
        .map(|i| {
            let top = (f64::from(i) * stride).round() as u32;
            (top, ((f64::from(top) + band).round() as u32).min(height))
        })
        .collect()
}

fn overlap_1d(a: (u32, u32), b: (u32, u32)) -> u32 {
    a.1.min(b.1).saturating_sub(a.0.max(b.0))
}

/// The same line found in two bands, whole in both or cut where a band
/// ends: written the same way and lying on the same track (most of the
/// narrower one's width for a column, height for a row) with their lengths
/// meeting. They become one box around both.
fn same_line(a: &[u32; 4], b: &[u32; 4]) -> bool {
    let (vertical_a, vertical_b) = (is_vertical_line(a), is_vertical_line(b));
    let area = |r: &[u32; 4]| u64::from(r[2] - r[0]) * u64::from(r[3] - r[1]);
    let shared = u64::from(overlap_1d((a[0], a[2]), (b[0], b[2]))) * u64::from(overlap_1d((a[1], a[3]), (b[1], b[3])));
    if shared * 10 >= 7 * area(a).min(area(b)) {
        return true;
    }
    if vertical_a != vertical_b {
        // A piece of a column cut short can look like a square: only a
        // near-copy joins then.
        return false;
    }
    let (track, length) = if vertical_a { ((0, 2), (1, 3)) } else { ((1, 3), (0, 2)) };
    let narrower = (a[track.1] - a[track.0]).min(b[track.1] - b[track.0]);
    let on_track = overlap_1d((a[track.0], a[track.1]), (b[track.0], b[track.1]));
    on_track * 10 >= 7 * narrower && overlap_1d((a[length.0], a[length.1]), (b[length.0], b[length.1])) > 0
}

/// Lines found in several bands, once each.
pub fn merge_lines(mut boxes: Vec<[u32; 4]>) -> Vec<[u32; 4]> {
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if same_line(&boxes[i], &boxes[j]) {
                    let (a, b) = (boxes[i], boxes.swap_remove(j));
                    boxes[i] = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    boxes
}

/// A line written top to bottom: taller than it is wide.
pub fn is_vertical_line(b: &[u32; 4]) -> bool {
    b[3] - b[1] > b[2] - b[0]
}

/// One input of a recognizer: a line, or a piece of a long column.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Which line it belongs to.
    pub line: usize,
    /// Where it is on the page, `[x1, y1, x2, y2]`.
    pub origin: [u32; 4],
    /// The size of its pixels inside the model's input (the rest is padding).
    pub effective: (u32, u32),
    pub tensor: Vec<f32>,
}

/// A horizontal line, as the 960×32 model takes it.
pub fn horizontal_segment(image: &DynamicImage, line: usize, b: [u32; 4]) -> Option<Segment> {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w == 0 || h == 0 {
        return None;
    }
    let mut new_h = REC_HEIGHT;
    let mut new_w = (f64::from(w) * f64::from(REC_HEIGHT) / f64::from(h)).round().max(1.0) as u32;
    if new_w > REC_WIDTH {
        new_h = ((f64::from(new_h) * f64::from(REC_WIDTH) / f64::from(new_w)).round() as u32).max(1);
        new_w = REC_WIDTH;
    }
    let crop = image.crop_imm(b[0], b[1], w, h).resize_exact(new_w, new_h, FilterType::Triangle);
    let plane = (REC_WIDTH * REC_HEIGHT) as usize;
    let mut tensor = vec![0f32; plane * 3];
    put_bgr(&crop, &mut tensor, REC_WIDTH as usize, plane);
    Some(Segment { line, origin: b, effective: (new_w, new_h), tensor })
}

/// Where the pieces of a column start (page pixels) and how high each is,
/// for a column `y1..y2` scaled by `scale` into the 32 px wide model. Only a
/// column too long for the model is cut.
pub fn column_pieces(y1: f64, y2: f64, scale: f64) -> (Vec<f64>, f64, f64) {
    let scaled = (y2 - y1) * scale;
    if scaled <= f64::from(VREC_HEIGHT) {
        return (vec![y1], y2 - y1, f64::from(VREC_HEIGHT));
    }
    let piece = VREC_SEGMENT / scale;
    let stride = (VREC_SEGMENT - VREC_OVERLAP) / scale;
    let mut starts = Vec::new();
    let mut y = y1;
    while y + piece < y2 {
        starts.push(y);
        y += stride;
    }
    let last = y2 - piece;
    if starts.last().is_none_or(|&previous| last > previous + 1.0) {
        starts.push(last);
    }
    (starts, piece, VREC_SEGMENT)
}

/// A vertical line, as the 32×480 model takes it: one piece, or several.
pub fn vertical_segments(image: &DynamicImage, line: usize, b: [u32; 4]) -> Vec<Segment> {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let scale = f64::from(VREC_WIDTH) / f64::from(w);
    let (starts, piece, max_h) = column_pieces(f64::from(b[1]), f64::from(b[3]), scale);
    let plane = (VREC_WIDTH * VREC_HEIGHT) as usize;
    starts
        .into_iter()
        .filter_map(|start| {
            let sy1 = (start.round().max(0.0) as u32).min(b[3]);
            let sy2 = ((start + piece).round() as u32).min(b[3]);
            if sy2 <= sy1 {
                return None;
            }
            let seg_h = sy2 - sy1;
            let new_h = ((f64::from(seg_h) * scale).round().min(max_h) as u32).max(1);
            let crop = image.crop_imm(b[0], sy1, w, seg_h).resize_exact(VREC_WIDTH, new_h, FilterType::Triangle);
            let mut tensor = vec![0f32; plane * 3];
            put_bgr(&crop, &mut tensor, VREC_WIDTH as usize, plane);
            Some(Segment { line, origin: [b[0], sy1, b[2], sy2], effective: (VREC_WIDTH, new_h), tensor })
        })
        .collect()
}

/// A character the recognizer found, placed along its line in page pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub ch: char,
    pub score: f32,
    /// Its extent along the line: x for a row, y for a column.
    pub interval: (i64, i64),
}

/// What a recognizer said about a segment, as characters on the page.
/// `boxes` are `[x1, y1, x2, y2]` in the model's pixels.
pub fn candidates(segment: &Segment, vertical: bool, codes: &[i32], boxes: &[f32], scores: &[f32], threshold: f32) -> Vec<Candidate> {
    let [gx1, gy1, gx2, gy2] = segment.origin.map(f64::from);
    let (crop_w, crop_h) = (gx2 - gx1, gy2 - gy1);
    let (ew, eh) = (f64::from(segment.effective.0), f64::from(segment.effective.1));
    codes
        .iter()
        .zip(scores)
        .zip(boxes.chunks_exact(4))
        .filter_map(|((&code, &score), b)| {
            if score < threshold || b.iter().any(|v| !v.is_finite()) {
                return None;
            }
            let ch = u32::try_from(code).ok().and_then(char::from_u32).filter(|c| !c.is_control())?;
            let b = b.iter().map(|&v| f64::from(v)).collect::<Vec<_>>();
            if vertical {
                if b[1] >= eh {
                    return None;
                }
                let y1 = gy1 + (b[1].min(eh) / eh * crop_h).trunc();
                let y2 = gy1 + (b[3].min(eh) / eh * crop_h).trunc();
                (y2 > y1).then_some(Candidate { ch, score, interval: (y1 as i64, y2 as i64) })
            } else {
                if b[0] >= ew {
                    return None;
                }
                let x1 = gx1 + (b[0].min(ew) / ew * crop_w).trunc();
                let x2 = gx1 + (b[2].min(ew) / ew * crop_w).trunc();
                Some(Candidate { ch, score, interval: (x1 as i64, x2 as i64) })
            }
        })
        .collect()
}

/// A line's text from every candidate of its segments: the most confident
/// character wherever two overlap, in order along the line.
pub fn pick(mut found: Vec<Candidate>) -> String {
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in found {
        let (c1, c2) = candidate.interval;
        let clashes = kept.iter().any(|k| {
            let (k1, k2) = k.interval;
            if c1 >= k2 || k1 >= c2 {
                return false;
            }
            let shared = (c2.min(k2) - c1.max(k1)).max(0) as f64;
            let shorter = ((c2 - c1) as f64).min((k2 - k1) as f64) + 1e-6;
            shared / shorter > SAME_CHARACTER
        });
        if !clashes {
            kept.push(candidate);
        }
    }
    kept.sort_by_key(|c| c.interval.0);
    let mut text: String = kept.iter().map(|c| c.ch).collect();
    for (wrong, right) in SWAPPED_PAIRS {
        if text.contains(wrong) {
            text = text.replacen(wrong, right, 1);
        }
    }
    text
}

/// A line read off the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub bbox: [f64; 4],
    pub vertical: bool,
    pub text: String,
}

fn span_overlap(a: (f64, f64), b: (f64, f64)) -> f64 {
    let shared = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
    let shorter = (a.1 - a.0).min(b.1 - b.0);
    if shorter <= 0.0 { 0.0 } else { shared / shorter }
}

/// Lines of the same bubble: written the same way, about the same size,
/// side by side (columns) or one above the other (rows), with a gap of at
/// most a character and most of their length shared.
pub fn same_block(a: &Line, b: &Line) -> bool {
    if a.vertical != b.vertical {
        return false;
    }
    let (a_thick, b_thick, gap, along) = if a.vertical {
        (
            a.bbox[2] - a.bbox[0],
            b.bbox[2] - b.bbox[0],
            a.bbox[0].max(b.bbox[0]) - a.bbox[2].min(b.bbox[2]),
            span_overlap((a.bbox[1], a.bbox[3]), (b.bbox[1], b.bbox[3])),
        )
    } else {
        (
            a.bbox[3] - a.bbox[1],
            b.bbox[3] - b.bbox[1],
            a.bbox[1].max(b.bbox[1]) - a.bbox[3].min(b.bbox[3]),
            span_overlap((a.bbox[0], a.bbox[2]), (b.bbox[0], b.bbox[2])),
        )
    };
    let (thin, thick) = (a_thick.min(b_thick), a_thick.max(b_thick));
    thin > 0.0 && thick / thin <= 1.6 && gap <= thick && along >= 0.5
}

/// Furigana: a line written the same way as another, much thinner (under
/// 60 % of its thickness), right beside it and within its length. meikiocr
/// reads it as a line of its own; mokuro and manga-ocr leave it out, and it
/// would be counted as text read twice, so it goes.
pub fn is_furigana(small: &Line, big: &Line) -> bool {
    if small.vertical != big.vertical {
        return false;
    }
    let (s, b) = (small.bbox, big.bbox);
    let (s_thick, b_thick, gap, along, s_len) = if big.vertical {
        (s[2] - s[0], b[2] - b[0], s[0].max(b[0]) - s[2].min(b[2]), (s[3].min(b[3]) - s[1].max(b[1])).max(0.0), s[3] - s[1])
    } else {
        (s[3] - s[1], b[3] - b[1], s[1].max(b[1]) - s[3].min(b[3]), (s[2].min(b[2]) - s[0].max(b[0])).max(0.0), s[2] - s[0])
    };
    s_thick > 0.0 && s_len > 0.0 && s_thick < 0.6 * b_thick && gap <= 0.35 * b_thick && gap >= -0.5 * s_thick && along >= 0.8 * s_len
}

/// The lines without their furigana.
pub fn without_furigana(lines: &[Line]) -> Vec<Line> {
    lines.iter().filter(|line| !lines.iter().any(|other| !std::ptr::eq(*line, other) && is_furigana(line, other))).cloned().collect()
}

/// Lines joined into `.mokuro` blocks: columns from the right, rows from
/// the top, each with its own corners. Blocks come in no particular order.
/// Furigana is left out.
pub fn blocks(lines: &[Line]) -> Vec<MokuroBlock> {
    let lines = without_furigana(lines);
    let lines: Vec<&Line> = lines.iter().filter(|line| !line.text.trim().is_empty()).collect();
    // Union-find over the pairs that belong together.
    let mut parent: Vec<usize> = (0..lines.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            if same_block(lines[i], lines[j]) {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri.max(rj)] = ri.min(rj);
            }
        }
    }
    let mut groups: Vec<Vec<&Line>> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; lines.len()];
    for i in 0..lines.len() {
        let r = root(&mut parent, i);
        let index = *slot[r].get_or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[index].push(lines[i]);
    }
    groups
        .into_iter()
        .map(|mut group| {
            let vertical = group[0].vertical;
            if vertical {
                group.sort_by(|a, b| (b.bbox[0] + b.bbox[2]).total_cmp(&(a.bbox[0] + a.bbox[2])));
            } else {
                group.sort_by(|a, b| (a.bbox[1] + a.bbox[3]).total_cmp(&(b.bbox[1] + b.bbox[3])));
            }
            let bbox = group.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, line| {
                [u[0].min(line.bbox[0]), u[1].min(line.bbox[1]), u[2].max(line.bbox[2]), u[3].max(line.bbox[3])]
            });
            let mut sizes: Vec<f64> = group
                .iter()
                .map(|line| if vertical { line.bbox[2] - line.bbox[0] } else { line.bbox[3] - line.bbox[1] })
                .collect();
            sizes.sort_by(f64::total_cmp);
            MokuroBlock {
                bbox,
                vertical,
                font_size: sizes[sizes.len() / 2].round().max(1.0),
                lines_coords: group
                    .iter()
                    .map(|line| {
                        let [x1, y1, x2, y2] = line.bbox;
                        vec![[x1, y1], [x2, y1], [x2, y2], [x1, y2]]
                    })
                    .collect(),
                lines: group.iter().map(|line| line.text.clone()).collect(),
            }
        })
        .collect()
}

/// The page as `.mokuro`: blocks in manga reading order.
pub fn page(img_path: &str, image: (u32, u32), lines: &[Line]) -> MokuroPage {
    let blocks = blocks(lines);
    let boxes: Vec<[f64; 4]> = blocks.iter().map(|block| block.bbox).collect();
    let mut slots: Vec<Option<MokuroBlock>> = blocks.into_iter().map(Some).collect();
    let blocks = super::layout::reading_order(&boxes).into_iter().filter_map(|index| slots[index].take()).collect();
    MokuroPage { img_path: img_path.to_string(), img_width: f64::from(image.0), img_height: f64::from(image.1), blocks }
}

/// The text of the lines whose centre is inside `inside`, in reading order.
pub fn text_inside(lines: &[Line], inside: [f64; 4]) -> String {
    let within: Vec<Line> = lines
        .iter()
        .filter(|line| {
            let (x, y) = ((line.bbox[0] + line.bbox[2]) / 2.0, (line.bbox[1] + line.bbox[3]) / 2.0);
            (inside[0]..=inside[2]).contains(&x) && (inside[1]..=inside[3]).contains(&y)
        })
        .cloned()
        .collect();
    page("", (1, 1), &within).blocks.iter().flat_map(|block| block.lines.iter().map(String::as_str)).collect()
}

/// The box the user drew, with a margin around it so the detector sees the
/// whole of lines that touch its edge, inside the page: `[x1, y1, x2, y2]`.
pub fn region_with_margin(image: (u32, u32), bbox: [f64; 4]) -> Option<[u32; 4]> {
    if bbox.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let (x1, y1, x2, y2) = (bbox[0].min(bbox[2]), bbox[1].min(bbox[3]), bbox[0].max(bbox[2]), bbox[1].max(bbox[3]));
    if x2 - x1 < 2.0 || y2 - y1 < 2.0 {
        return None;
    }
    let margin = (0.25 * (x2 - x1).max(y2 - y1)).max(24.0);
    let (w, h) = (f64::from(image.0), f64::from(image.1));
    let out = [
        (x1 - margin).clamp(0.0, w).floor() as u32,
        (y1 - margin).clamp(0.0, h).floor() as u32,
        (x2 + margin).clamp(0.0, w).ceil() as u32,
        (y2 + margin).clamp(0.0, h).ceil() as u32,
    ];
    (out[2] > out[0] + 1 && out[3] > out[1] + 1).then_some(out)
}

pub struct Engine {
    detector: Session,
    horizontal: Session,
    vertical: Session,
    pub provider: Provider,
    dir: PathBuf,
}

impl Engine {
    /// The runtime and meikiocr's models from `dir`, on the accelerator if
    /// it starts, on the CPU otherwise.
    pub fn load(dir: &Path) -> Result<Engine, String> {
        let runtime = assets::runtime().ok_or("unsupported")?;
        let models = assets::meiki_models();
        if let Some(missing) = runtime.iter().chain(&models).find(|asset| !asset.present(dir)) {
            return Err(format!("missing {}", missing.file));
        }
        environment(dir, runtime)?;
        if let Some(name) = crate::platform::ocr_accelerator() {
            match Self::sessions(dir, true) {
                Ok(engine) => return Ok(Engine { provider: Provider::Accelerated(name), ..engine }),
                Err(error) => log::warn!("meikiocr: {name} did not start ({error}); using the CPU"),
            }
        }
        Self::sessions(dir, false)
    }

    fn sessions(dir: &Path, accelerated: bool) -> Result<Engine, String> {
        let environment = environment(dir, assets::runtime().ok_or("unsupported")?)?;
        Ok(Engine {
            detector: session(environment, &dir.join(assets::MEIKI_DETECTOR.file), accelerated)?,
            horizontal: session(environment, &dir.join(assets::MEIKI_HORIZONTAL.file), accelerated)?,
            vertical: session(environment, &dir.join(assets::MEIKI_VERTICAL.file), accelerated)?,
            provider: Provider::Cpu,
            dir: dir.to_path_buf(),
        })
    }

    /// Runs `f`, and on the accelerator, once more on the CPU if it fails.
    fn with_fallback<T>(&mut self, mut f: impl FnMut(&mut Engine) -> Result<T, String>) -> Result<T, String> {
        match f(self) {
            Err(error) if self.provider != Provider::Cpu => {
                log::warn!("meikiocr: {} failed ({error}); moving to the CPU", self.provider.name());
                *self = Self::sessions(&self.dir.clone(), false)?;
                f(self)
            }
            other => other,
        }
    }

    /// The text of a page, as a `.mokuro` page.
    pub fn recognize_page(&mut self, image: &DynamicImage, img_path: &str) -> Result<MokuroPage, String> {
        self.with_fallback(|engine| {
            let lines = engine.read(image)?;
            Ok(page(img_path, image.dimensions(), &lines))
        })
    }

    /// The text inside a box the user drew: the lines found around it whose
    /// centre is inside, or the box read as one line if none is.
    pub fn recognize_box(&mut self, image: &DynamicImage, bbox: [f64; 4]) -> Result<String, String> {
        let area = region_with_margin(image.dimensions(), bbox).ok_or("bad-box")?;
        let crop = image.crop_imm(area[0], area[1], area[2] - area[0], area[3] - area[1]);
        let inside = [
            bbox[0].min(bbox[2]) - f64::from(area[0]),
            bbox[1].min(bbox[3]) - f64::from(area[1]),
            bbox[0].max(bbox[2]) - f64::from(area[0]),
            bbox[1].max(bbox[3]) - f64::from(area[1]),
        ];
        self.with_fallback(|engine| {
            let text = text_inside(&engine.read(&crop)?, inside);
            if !text.is_empty() {
                return Ok(text);
            }
            let whole = inside.map(|v| v.max(0.0).round() as u32);
            let whole = [whole[0], whole[1], whole[2].min(crop.width()), whole[3].min(crop.height())];
            Ok(engine.read_lines(&crop, &[whole])?.into_iter().map(|line| line.text).collect())
        })
    }

    /// Every line of an image, read.
    fn read(&mut self, image: &DynamicImage) -> Result<Vec<Line>, String> {
        let boxes = self.detect(image)?;
        self.read_lines(image, &boxes)
    }

    /// The lines of an image, band by band when it is taller than the
    /// detector's shape (see `bands`).
    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<[u32; 4]>, String> {
        let (w, h) = image.dimensions();
        let mut found = Vec::new();
        for (top, bottom) in bands(w, h) {
            let band = image.crop_imm(0, top, w, bottom - top);
            found.extend(self.detect_band(&band)?.into_iter().map(|b| [b[0], b[1] + top, b[2], b[3] + top]));
        }
        let mut merged = merge_lines(found);
        merged.sort_by_key(|b| b[1]);
        Ok(merged)
    }

    fn detect_band(&mut self, image: &DynamicImage) -> Result<Vec<[u32; 4]>, String> {
        let (pixels, sizes) = detector_input(image);
        let images = Tensor::from_array(([1usize, 3, DET_HEIGHT as usize, DET_WIDTH as usize], pixels)).map_err(|e| e.to_string())?;
        let sizes = Tensor::from_array(([1usize, 2], sizes.to_vec())).map_err(|e| e.to_string())?;
        let outputs = self.detector.run(ort::inputs!["images" => images, "orig_target_sizes" => sizes]).map_err(|e| e.to_string())?;
        let (_, boxes) = outputs["boxes"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (_, scores) = outputs["scores"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        Ok(line_boxes(boxes, scores, image.dimensions(), DET_THRESHOLD))
    }

    fn read_lines(&mut self, image: &DynamicImage, boxes: &[[u32; 4]]) -> Result<Vec<Line>, String> {
        let mut found: Vec<Vec<Candidate>> = vec![Vec::new(); boxes.len()];
        for vertical in [false, true] {
            let segments: Vec<Segment> = boxes
                .iter()
                .enumerate()
                .filter(|(_, b)| is_vertical_line(b) == vertical)
                .flat_map(|(line, &b)| if vertical { vertical_segments(image, line, b) } else { horizontal_segment(image, line, b).into_iter().collect() })
                .collect();
            for chunk in segments.chunks(BATCH) {
                for (segment, candidates) in chunk.iter().zip(self.recognize(chunk, vertical)?) {
                    found[segment.line].extend(candidates);
                }
            }
        }
        Ok(boxes
            .iter()
            .zip(found)
            .map(|(b, candidates)| Line {
                bbox: b.map(f64::from),
                vertical: is_vertical_line(b),
                text: super::text::post_process(&pick(candidates)),
            })
            .filter(|line| !line.text.is_empty())
            .collect())
    }

    /// Runs a recognizer on segments of one orientation.
    fn recognize(&mut self, segments: &[Segment], vertical: bool) -> Result<Vec<Vec<Candidate>>, String> {
        let (w, h, session) = if vertical { (VREC_WIDTH, VREC_HEIGHT, &mut self.vertical) } else { (REC_WIDTH, REC_HEIGHT, &mut self.horizontal) };
        let batch = segments.len();
        let pixels: Vec<f32> = segments.iter().flat_map(|segment| segment.tensor.iter().copied()).collect();
        let images = Tensor::from_array(([batch, 3, h as usize, w as usize], pixels)).map_err(|e| e.to_string())?;
        let sizes = Tensor::from_array(([1usize, 2], vec![i64::from(w), i64::from(h)])).map_err(|e| e.to_string())?;
        let outputs = session.run(ort::inputs!["images" => images, "orig_target_sizes" => sizes]).map_err(|e| e.to_string())?;
        let (shape, codes) = outputs["char_codes"].try_extract_tensor::<i32>().map_err(|e| e.to_string())?;
        let (_, boxes) = outputs["boxes"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (_, scores) = outputs["scores"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let per = shape.get(1).copied().unwrap_or(0).max(0) as usize;
        if codes.len() < batch * per || scores.len() < batch * per || boxes.len() < batch * per * 4 {
            return Err("meikiocr: short output".into());
        }
        Ok(segments
            .iter()
            .enumerate()
            .map(|(i, segment)| {
                let range = i * per..(i + 1) * per;
                candidates(segment, vertical, &codes[range.clone()], &boxes[range.start * 4..range.end * 4], &scores[range], REC_THRESHOLD)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(bbox: [f64; 4], vertical: bool, text: &str) -> Line {
        Line { bbox, vertical, text: text.into() }
    }

    #[test]
    fn the_page_goes_to_the_top_left_of_the_detector_as_bgr() {
        // A portrait page: scaled to 544 high, so 272 wide.
        let mut page = image::RgbImage::from_pixel(500, 1000, image::Rgb([10, 20, 30]));
        page.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        let (tensor, sizes) = detector_input(&DynamicImage::ImageRgb8(page));
        let plane = (DET_WIDTH * DET_HEIGHT) as usize;
        assert_eq!(tensor.len(), 3 * plane);
        // The target size undoes the scale: boxes come back in page pixels.
        assert_eq!(sizes[0], 1764);
        assert!((999..=1000).contains(&sizes[1]), "{sizes:?}");
        // Blue first. The corner pixel is mostly red, blurred a little.
        let middle = 100 * DET_WIDTH as usize + 100;
        assert!((tensor[middle] - 30.0 / 255.0).abs() < 1e-4);
        assert!((tensor[2 * plane + middle] - 10.0 / 255.0).abs() < 1e-4);
        assert!(tensor[2 * plane] > tensor[0]);
        // Past the page: black padding.
        assert_eq!(tensor[100 * DET_WIDTH as usize + 300], 0.0);
        assert_eq!(tensor[600], 0.0);
    }

    #[test]
    fn lines_are_confident_inside_and_top_first() {
        let boxes = [10.0, 300.0, 50.0, 500.0, 0.0, 0.0, 0.0, 0.0, -5.0, 20.0, 700.0, 60.0, 1.0, 1.0, 2.0, 2.0];
        let scores = [0.9, 0.2, 0.6, 0.51];
        let found = line_boxes(&boxes, &scores, (600, 800), 0.5);
        assert_eq!(found, [[1, 1, 2, 2], [0, 20, 600, 60], [10, 300, 50, 500]]);
        assert!(is_vertical_line(&found[2]));
        assert!(!is_vertical_line(&found[1]));
    }

    #[test]
    fn a_long_column_is_read_in_overlapping_pieces() {
        // Fits: one piece, the whole column.
        assert_eq!(column_pieces(100.0, 500.0, 1.0), (vec![100.0], 400.0, 480.0));
        // 1000 px at scale 1: pieces of 420 every 356, the last one flush
        // with the end.
        let (starts, piece, max_h) = column_pieces(0.0, 1000.0, 1.0);
        assert_eq!((piece, max_h), (420.0, 420.0));
        assert_eq!(starts, [0.0, 356.0, 580.0]);
        assert!(starts.windows(2).all(|w| w[0] + piece > w[1]), "they overlap");
        // At half scale the pieces are twice as long in page pixels.
        let (starts, piece, _) = column_pieces(0.0, 2000.0, 0.5);
        assert_eq!(piece, 840.0);
        assert_eq!(*starts.last().unwrap() + piece, 2000.0);
    }

    #[test]
    fn segments_are_padded_like_the_models_want() {
        let page = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(400, 2000, image::Rgb([255, 255, 255])));
        let row = horizontal_segment(&page, 0, [0, 0, 320, 40]).unwrap();
        assert_eq!(row.effective, (256, 32));
        assert_eq!(row.tensor.len(), 3 * 960 * 32);
        assert_eq!(row.tensor[255], 1.0);
        assert_eq!(row.tensor[256], 0.0);
        // Far too wide: 960 wide, less than 32 high.
        let wide = horizontal_segment(&page, 0, [0, 0, 400, 10]).unwrap();
        assert_eq!(wide.effective, (960, 24));
        let column = vertical_segments(&page, 3, [100, 0, 140, 300]);
        assert_eq!(column.len(), 1);
        assert_eq!(column[0].effective, (32, 240));
        assert_eq!(column[0].line, 3);
        let long = vertical_segments(&page, 0, [100, 0, 132, 1500]);
        assert_eq!(long.len(), 5);
        assert!(long.iter().all(|s| s.effective.1 <= 420));
        assert_eq!(long.last().unwrap().origin[3], 1500);
    }

    #[test]
    fn characters_go_back_to_the_page_and_the_best_one_wins() {
        let segment = Segment { line: 0, origin: [100, 50, 420, 90], effective: (256, 32), tensor: Vec::new() };
        let codes = ['東' as i32, '京' as i32, '亰' as i32, 'x' as i32, 0x110000];
        let boxes = [
            0.0, 0.0, 32.0, 32.0, //
            32.0, 0.0, 64.0, 32.0, //
            34.0, 0.0, 66.0, 32.0, // the same place as 京, less sure
            300.0, 0.0, 330.0, 32.0, // in the padding
            64.0, 0.0, 96.0, 32.0, // not a character
        ];
        let scores = [0.9, 0.8, 0.5, 0.9, 0.9];
        let found = candidates(&segment, false, &codes, &boxes, &scores, 0.1);
        assert_eq!(found.len(), 3);
        // 32 model px are 40 page px here.
        assert_eq!(found[0].interval, (100, 140));
        assert_eq!(pick(found), "東京");
        assert_eq!(pick(vec![Candidate { ch: '儡', score: 0.9, interval: (0, 10) }, Candidate { ch: '傀', score: 0.9, interval: (10, 20) }]), "傀儡");

        // A column: along y.
        let piece = Segment { line: 0, origin: [10, 200, 50, 500], effective: (32, 240), tensor: Vec::new() };
        let found = candidates(&piece, true, &['あ' as i32], &[0.0, 120.0, 32.0, 150.0], &[0.7], 0.1);
        assert_eq!(found[0].interval, (350, 387));
        assert!(candidates(&piece, true, &['あ' as i32], &[0.0, 250.0, 32.0, 270.0], &[0.7], 0.1).is_empty());
        assert!(candidates(&piece, true, &['あ' as i32], &[0.0, 0.0, 32.0, 20.0], &[0.05], 0.1).is_empty());
    }

    #[test]
    fn columns_of_a_bubble_become_one_block_read_from_the_right() {
        let lines = [
            line([900.0, 110.0, 960.0, 470.0], true, "今日はいい"),
            line([830.0, 110.0, 890.0, 330.0], true, "天気だね"),
            // Another bubble, further left.
            line([520.0, 200.0, 580.0, 560.0], true, "そうだね"),
            // A caption.
            line([100.0, 930.0, 430.0, 990.0], false, "東京駅で会おう"),
            line([110.0, 1000.0, 300.0, 1055.0], false, "またね"),
            // Furigana beside a column: dropped.
            line([962.0, 120.0, 980.0, 200.0], true, "きょう"),
            // Columns a whole character apart still make one block.
            line([300.0, 600.0, 340.0, 900.0], true, "ここは"),
            line([222.0, 600.0, 262.0, 800.0], true, "どこ"),
        ];
        let page = page("p.jpg", (1100, 1300), &lines);
        let read: Vec<String> = page.blocks.iter().map(|b| b.lines.join("/")).collect();
        assert_eq!(read, ["今日はいい/天気だね", "そうだね", "ここは/どこ", "東京駅で会おう/またね"]);
        let bubble = &page.blocks[0];
        assert!(bubble.vertical);
        assert_eq!(bubble.bbox, [830.0, 110.0, 960.0, 470.0]);
        assert_eq!(bubble.lines_coords[0][0], [900.0, 110.0]);
        assert_eq!(bubble.font_size, 60.0);
        assert!(!page.blocks[3].vertical);
        // Two characters apart: two blocks.
        let apart = blocks(&[line([300.0, 600.0, 340.0, 900.0], true, "あ"), line([180.0, 600.0, 220.0, 800.0], true, "い")]);
        assert_eq!(apart.len(), 2);
    }

    #[test]
    fn a_tall_page_is_detected_in_overlapping_bands() {
        // A game's screen: one band.
        assert_eq!(bands(1920, 1080), [(0, 1080)]);
        assert_eq!(bands(1200, 690), [(0, 690)]);
        // A manga page: bands of the detector's shape (1200 × 680), a
        // quarter shared, the last flush with the bottom.
        let page = bands(1200, 1700);
        assert_eq!(page.len(), 3);
        assert_eq!(page[0], (0, 680));
        assert_eq!(page.last().unwrap().1, 1700);
        assert!(page.windows(2).all(|w| w[1].0 < w[0].1 && w[0].1 - w[1].0 >= 170), "{page:?}");
        assert!(page.iter().all(|(top, bottom)| bottom - top == 680));
    }

    #[test]
    fn a_line_found_twice_is_kept_once() {
        let merged = merge_lines(vec![
            // A column cut where the first band ends, and its rest below.
            [500, 300, 540, 680],
            [501, 520, 541, 900],
            // The next column, not the same line.
            [440, 300, 480, 680],
            // The same caption twice, a pixel off.
            [100, 600, 400, 640],
            [101, 601, 400, 641],
        ]);
        let mut merged = merged;
        merged.sort();
        assert_eq!(merged, [[100, 600, 400, 641], [440, 300, 480, 680], [500, 300, 541, 900]]);
    }

    #[test]
    fn furigana_is_left_out() {
        let lines = [
            line([900.0, 100.0, 940.0, 500.0], true, "昔、この町には"),
            // Its ruby, thin, just right of it.
            line([941.0, 100.0, 958.0, 150.0], true, "むかし"),
            // The next column, as thick: kept.
            line([840.0, 100.0, 880.0, 400.0], true, "図書館があった。"),
            // Small text on its own: kept.
            line([300.0, 100.0, 318.0, 200.0], true, "ざわざわ"),
            // Ruby over a horizontal line.
            line([100.0, 760.0, 160.0, 778.0], false, "としょ"),
            line([90.0, 780.0, 400.0, 820.0], false, "図書館"),
        ];
        let kept: Vec<String> = without_furigana(&lines).into_iter().map(|line| line.text).collect();
        assert_eq!(kept, ["昔、この町には", "図書館があった。", "ざわざわ", "図書館"]);
    }

    #[test]
    fn a_drawn_box_keeps_the_lines_inside_it() {
        let lines = [line([100.0, 0.0, 140.0, 200.0], true, "右"), line([40.0, 0.0, 80.0, 200.0], true, "左"), line([300.0, 0.0, 340.0, 200.0], true, "外")];
        assert_eq!(text_inside(&lines, [0.0, 0.0, 200.0, 300.0]), "右左");
        assert_eq!(region_with_margin((400, 600), [300.0, 300.0, 200.0, 100.0]), Some([150, 50, 350, 350]));
        assert_eq!(region_with_margin((400, 600), [380.0, 0.0, 400.0, 50.0]), Some([356, 0, 400, 74]));
        assert!(region_with_margin((400, 600), [10.0, 10.0, 10.5, 10.5]).is_none());
        assert!(region_with_margin((400, 600), [f64::NAN, 0.0, 10.0, 10.0]).is_none());
    }

    /// meikiocr on a real page, with the real runtime and models. Needs
    /// `NTDC_OCR_DIR` (the runtime and `assets::meiki_models`) and
    /// `NTDC_OCR_PAGE`; prints what it read.
    #[test]
    #[ignore]
    fn reads_a_real_page_with_meikiocr() {
        let dir = PathBuf::from(std::env::var("NTDC_OCR_DIR").expect("NTDC_OCR_DIR"));
        let page = std::env::var("NTDC_OCR_PAGE").expect("NTDC_OCR_PAGE");
        let started = std::time::Instant::now();
        let mut engine = Engine::load(&dir).unwrap();
        let loaded = started.elapsed();
        let image = image::open(&page).unwrap();
        // The first run warms the sessions up; the second is the one timed.
        let _ = engine.recognize_page(&image, "page.jpg").unwrap();
        let started = std::time::Instant::now();
        let result = engine.recognize_page(&image, "page.jpg").unwrap();
        let elapsed = started.elapsed();
        println!("provider: {}, load {:?}, page {:?}", engine.provider.name(), loaded, elapsed);
        for block in &result.blocks {
            println!("{:?} vertical={} lines={:?}", block.bbox.map(|v| v.round()), block.vertical, block.lines);
        }
        if let Ok(out) = std::env::var("NTDC_OCR_OUT") {
            std::fs::write(out, serde_json::to_string_pretty(&result).unwrap()).unwrap();
        }
        if let Ok(expected) = std::env::var("NTDC_OCR_EXPECT") {
            let read: Vec<String> = result.blocks.iter().map(|block| block.lines.concat()).collect();
            assert_eq!(read.join("|"), expected);
        }
    }
}
