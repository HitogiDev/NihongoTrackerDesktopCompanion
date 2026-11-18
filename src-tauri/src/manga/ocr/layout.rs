//! From what the detector finds to the blocks of a `.mokuro` page: which
//! boxes are text, which way they are written, in which order a reader of
//! manga takes them, and how their text is laid out in lines.
//!
//! Pure functions: the engine feeds them, the tests use made-up boxes.

use super::super::mokuro::{MokuroBlock, MokuroPage};

/// The detector's classes (`ogkalu/comic-text-and-bubble-detector`).
pub const LABEL_BUBBLE: i64 = 0;
pub const LABEL_TEXT_BUBBLE: i64 = 1;
pub const LABEL_TEXT_FREE: i64 = 2;

/// Below this a detection is noise. RT-DETR scores are sigmoid
/// probabilities; real text on a page scores well above it.
pub const MIN_SCORE: f32 = 0.4;
/// Smaller boxes (in image pixels) cannot hold a legible character.
const MIN_SIDE: f64 = 8.0;
/// Two boxes this much the same are one detection.
const SAME_BOX: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub label: i64,
    pub score: f32,
    /// `[x1, y1, x2, y2]` in image pixels.
    pub bbox: [f64; 4],
}

fn width(b: &[f64; 4]) -> f64 {
    (b[2] - b[0]).max(0.0)
}
fn height(b: &[f64; 4]) -> f64 {
    (b[3] - b[1]).max(0.0)
}
fn area(b: &[f64; 4]) -> f64 {
    width(b) * height(b)
}
fn intersection(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    w * h
}

/// Text boxes worth reading: text classes above the score, inside the page,
/// big enough, and one box where the detector found the same text twice
/// (the most confident one, or the larger when one holds the other).
pub fn text_boxes(detections: &[Detection], image: (u32, u32)) -> Vec<[f64; 4]> {
    let (w, h) = (f64::from(image.0), f64::from(image.1));
    let mut found: Vec<Detection> = detections
        .iter()
        .filter(|d| (d.label == LABEL_TEXT_BUBBLE || d.label == LABEL_TEXT_FREE) && d.score >= MIN_SCORE)
        .filter(|d| d.bbox.iter().all(|v| v.is_finite()))
        .map(|d| Detection { bbox: [d.bbox[0].clamp(0.0, w), d.bbox[1].clamp(0.0, h), d.bbox[2].clamp(0.0, w), d.bbox[3].clamp(0.0, h)], ..*d })
        .filter(|d| width(&d.bbox) >= MIN_SIDE && height(&d.bbox) >= MIN_SIDE)
        .collect();
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<[f64; 4]> = Vec::new();
    for detection in found {
        let b = detection.bbox;
        let duplicate = kept.iter_mut().find(|k| {
            let shared = intersection(k, &b);
            shared >= SAME_BOX * area(&b).min(area(k))
        });
        match duplicate {
            // One inside the other: the text is the larger box.
            Some(k) => {
                if area(&b) > 1.5 * area(k) && intersection(k, &b) >= 0.9 * area(k) {
                    *k = b;
                }
            }
            None => kept.push(b),
        }
    }
    kept
}

/// Japanese in a box taller than it is wide is written in columns.
pub fn is_vertical(bbox: &[f64; 4]) -> bool {
    height(bbox) > width(bbox)
}

/// Overlap of two vertical spans, relative to the shorter one.
fn vertical_overlap(a: (f64, f64), b: (f64, f64)) -> f64 {
    let shared = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
    let shorter = (a.1 - a.0).min(b.1 - b.0);
    if shorter <= 0.0 { 0.0 } else { shared / shorter }
}

/// Boxes that share this much height are side by side, read right to left.
const SAME_ROW: f64 = 0.25;

/// Indices of `boxes` in manga reading order: top to bottom, and right to
/// left among boxes side by side (whose heights overlap).
pub fn reading_order(boxes: &[[f64; 4]]) -> Vec<usize> {
    let mut by_top: Vec<usize> = (0..boxes.len()).collect();
    by_top.sort_by(|&a, &b| boxes[a][1].total_cmp(&boxes[b][1]).then(boxes[b][2].total_cmp(&boxes[a][2])));
    // Rows: a box joins the first row it shares enough height with.
    let mut rows: Vec<(f64, f64, Vec<usize>)> = Vec::new();
    for index in by_top {
        let b = &boxes[index];
        let span = (b[1], b[3]);
        match rows.iter_mut().find(|(top, bottom, _)| vertical_overlap((*top, *bottom), span) >= SAME_ROW) {
            Some(row) => {
                row.0 = row.0.min(span.0);
                row.1 = row.1.max(span.1);
                row.2.push(index);
            }
            None => rows.push((span.0, span.1, vec![index])),
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter()
        .flat_map(|(_, _, mut members)| {
            members.sort_by(|&a, &b| boxes[b][2].total_cmp(&boxes[a][2]).then(boxes[a][1].total_cmp(&boxes[b][1])));
            members
        })
        .collect()
}

/// A recognized box as a `.mokuro` block. manga-ocr reads a box as one
/// string; it is split into as many lines (columns when vertical) as the box
/// seems to hold, so the reader's text layer looks like the page.
pub fn block(bbox: [f64; 4], text: &str) -> MokuroBlock {
    let vertical = is_vertical(&bbox);
    let chars: Vec<char> = text.chars().collect();
    let count = chars.len().max(1) as f64;
    let (w, h) = (width(&bbox), height(&bbox));
    let (along, across) = if vertical { (h, w) } else { (w, h) };
    // Characters fill the box: `count ≈ (along / size) × (across / (1.2 × size))`.
    let size = ((along * across) / (1.2 * count)).sqrt().min(across).min(along).max(1.0);
    let lines = ((across / (1.2 * size)).round() as usize).clamp(1, chars.len().max(1));
    let per_line = chars.len().div_ceil(lines).max(1);
    let lines: Vec<String> = chars.chunks(per_line).map(|chunk| chunk.iter().collect()).collect();
    let n = lines.len().max(1) as f64;
    let lines_coords = (0..lines.len())
        .map(|i| {
            let i = i as f64;
            if vertical {
                // Columns from the right.
                let x2 = bbox[2] - w * i / n;
                let x1 = bbox[2] - w * (i + 1.0) / n;
                vec![[x1, bbox[1]], [x2, bbox[1]], [x2, bbox[3]], [x1, bbox[3]]]
            } else {
                let y1 = bbox[1] + h * i / n;
                let y2 = bbox[1] + h * (i + 1.0) / n;
                vec![[bbox[0], y1], [bbox[2], y1], [bbox[2], y2], [bbox[0], y2]]
            }
        })
        .collect();
    MokuroBlock { bbox, vertical, font_size: size.round(), lines, lines_coords }
}

/// A page from its recognized boxes, already in reading order. Boxes where
/// nothing was read are left out.
pub fn page(img_path: &str, image: (u32, u32), blocks: Vec<([f64; 4], String)>) -> MokuroPage {
    MokuroPage {
        img_path: img_path.to_string(),
        img_width: f64::from(image.0),
        img_height: f64::from(image.1),
        blocks: blocks.into_iter().filter(|(_, text)| !text.trim().is_empty()).map(|(bbox, text)| block(bbox, &text)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(label: i64, score: f32, bbox: [f64; 4]) -> Detection {
        Detection { label, score, bbox }
    }

    #[test]
    fn only_confident_text_is_kept_once() {
        let found = text_boxes(
            &[
                d(LABEL_BUBBLE, 0.99, [0.0, 0.0, 400.0, 400.0]),
                d(LABEL_TEXT_BUBBLE, 0.9, [100.0, 100.0, 200.0, 300.0]),
                // The same text again, a little off.
                d(LABEL_TEXT_FREE, 0.5, [102.0, 98.0, 199.0, 305.0]),
                d(LABEL_TEXT_FREE, 0.2, [500.0, 100.0, 600.0, 300.0]),
                d(LABEL_TEXT_FREE, 0.8, [700.0, 100.0, 703.0, 300.0]),
                // Past the page's edge: clamped.
                d(LABEL_TEXT_FREE, 0.7, [900.0, -20.0, 1100.0, 60.0]),
            ],
            (1000, 1400),
        );
        assert_eq!(found, [[100.0, 100.0, 200.0, 300.0], [900.0, 0.0, 1000.0, 60.0]]);
    }

    #[test]
    fn a_box_inside_another_gives_way_to_it() {
        let found = text_boxes(
            &[d(LABEL_TEXT_BUBBLE, 0.9, [110.0, 110.0, 150.0, 200.0]), d(LABEL_TEXT_BUBBLE, 0.8, [100.0, 100.0, 200.0, 300.0])],
            (1000, 1000),
        );
        assert_eq!(found, [[100.0, 100.0, 200.0, 300.0]]);
    }

    #[test]
    fn tall_boxes_are_vertical() {
        assert!(is_vertical(&[0.0, 0.0, 40.0, 200.0]));
        assert!(!is_vertical(&[0.0, 0.0, 200.0, 40.0]));
        // A square (a single character) reads as horizontal.
        assert!(!is_vertical(&[0.0, 0.0, 50.0, 50.0]));
    }

    #[test]
    fn side_by_side_reads_right_to_left_then_down() {
        // Two bubbles at the top, staggered; two at the bottom; one tall one
        // spanning on the left.
        let boxes = [
            [100.0, 120.0, 250.0, 400.0], // top left, a bit lower
            [700.0, 80.0, 850.0, 380.0],  // top right
            [120.0, 900.0, 400.0, 980.0], // bottom left, a caption
            [700.0, 950.0, 800.0, 1250.0], // bottom right
            [400.0, 100.0, 500.0, 300.0], // top middle
        ];
        assert_eq!(reading_order(&boxes), [1, 4, 0, 3, 2]);
    }

    #[test]
    fn stacked_boxes_read_top_down() {
        let boxes = [[100.0, 600.0, 300.0, 700.0], [100.0, 100.0, 300.0, 200.0], [120.0, 350.0, 280.0, 450.0]];
        assert_eq!(reading_order(&boxes), [1, 2, 0]);
        assert!(reading_order(&[]).is_empty());
    }

    #[test]
    fn text_is_laid_out_in_the_boxs_columns() {
        // Two columns of five characters at ~50 px.
        let block = block([500.0, 100.0, 620.0, 350.0], "そうだね散歩しよう！");
        assert!(block.vertical);
        assert_eq!(block.lines, ["そうだね散", "歩しよう！"]);
        assert_eq!(block.lines.concat(), "そうだね散歩しよう！");
        // The first column is the right one.
        assert!(block.lines_coords[0][0][0] > block.lines_coords[1][0][0]);
        assert!((40.0..=60.0).contains(&block.font_size), "{}", block.font_size);

        let caption = super::block([100.0, 900.0, 430.0, 960.0], "東京駅で会おう");
        assert!(!caption.vertical);
        assert_eq!(caption.lines, ["東京駅で会おう"]);
    }

    #[test]
    fn empty_readings_are_dropped() {
        let page = page("001.jpg", (800, 1200), vec![([0.0, 0.0, 10.0, 40.0], " ".into()), ([0.0, 0.0, 40.0, 80.0], "あ".into())]);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.img_width, 800.0);
    }
}
