//! The picture that goes with a mined block: its box on the page, widened to
//! take in the panel around the speech bubble, as a PNG.
//!
//! Boxes come in the `.mokuro`'s pixels (`img_width` × `img_height`); the page
//! may have been resized since, so they are scaled to the image as decoded.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};

/// The margin around a block, as a share of its longer side: enough to show
/// the panel the bubble is in, not the whole page.
const MARGIN_SHARE: f64 = 0.6;
/// Never less than this, in image pixels, so a one-word bubble keeps context.
const MIN_MARGIN: f64 = 48.0;

/// `(x, y, width, height)` of the crop in the decoded image, or `None` for a
/// box that is empty or entirely off the page.
pub fn panel_rect(bbox: [f64; 4], text_size: (f64, f64), image: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (image_w, image_h) = (image.0 as f64, image.1 as f64);
    if image_w <= 0.0 || image_h <= 0.0 {
        return None;
    }
    let scale_x = if text_size.0 > 0.0 { image_w / text_size.0 } else { 1.0 };
    let scale_y = if text_size.1 > 0.0 { image_h / text_size.1 } else { 1.0 };
    let [x1, y1, x2, y2] = bbox;
    let (left, right) = (x1.min(x2) * scale_x, x1.max(x2) * scale_x);
    let (top, bottom) = (y1.min(y2) * scale_y, y1.max(y2) * scale_y);
    if !(right > left && bottom > top) || right <= 0.0 || bottom <= 0.0 || left >= image_w || top >= image_h {
        return None;
    }
    let margin = ((right - left).max(bottom - top) * MARGIN_SHARE).max(MIN_MARGIN);
    let x = (left - margin).max(0.0).floor();
    let y = (top - margin).max(0.0).floor();
    let w = ((right + margin).min(image_w).ceil() - x).max(1.0);
    let h = ((bottom + margin).min(image_h).ceil() - y).max(1.0);
    Some((x as u32, y as u32, w as u32, h as u32))
}

/// The crop of `page` (any format the reader shows) around `bbox`, as PNG.
pub fn crop_png(page: &[u8], bbox: [f64; 4], text_size: (f64, f64)) -> Result<Vec<u8>, String> {
    let mut reader = ImageReader::new(Cursor::new(page)).with_guessed_format().map_err(|e| e.to_string())?;
    reader.limits(super::thumbs::limits());
    let image = reader.decode().map_err(|e| e.to_string())?;
    let (x, y, w, h) = panel_rect(bbox, text_size, (image.width(), image.height())).ok_or("the block is not on the page")?;
    let mut out = Vec::new();
    image.crop_imm(x, y, w, h).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manga::thumbs::tests::png;

    #[test]
    fn the_block_grows_by_a_panel_margin_and_stays_on_the_page() {
        // 100×300 block: margin 180 on every side.
        assert_eq!(panel_rect([500.0, 400.0, 600.0, 700.0], (800.0, 1200.0), (800, 1200)), Some((320, 220, 460, 660)));
        // Near the corner: clamped to the image.
        assert_eq!(panel_rect([10.0, 10.0, 40.0, 30.0], (800.0, 1200.0), (800, 1200)), Some((0, 0, 88, 78)));
    }

    #[test]
    fn boxes_follow_a_page_that_was_resized_after_the_ocr() {
        // mokuro read it at 1600×2400; the file is half that now.
        let resized = panel_rect([1000.0, 800.0, 1200.0, 1400.0], (1600.0, 2400.0), (800, 1200)).unwrap();
        assert_eq!(resized, panel_rect([500.0, 400.0, 600.0, 700.0], (800.0, 1200.0), (800, 1200)).unwrap());
    }

    #[test]
    fn empty_or_off_page_boxes_have_no_crop() {
        assert_eq!(panel_rect([5.0, 5.0, 5.0, 50.0], (800.0, 1200.0), (800, 1200)), None);
        assert_eq!(panel_rect([900.0, 10.0, 950.0, 50.0], (800.0, 1200.0), (800, 1200)), None);
        assert_eq!(panel_rect([1.0, 1.0, 2.0, 2.0], (800.0, 1200.0), (0, 0)), None);
    }

    #[test]
    fn the_crop_is_a_png_of_that_size() {
        let out = crop_png(&png(800, 1200), [500.0, 400.0, 600.0, 700.0], (800.0, 1200.0)).unwrap();
        assert_eq!(&out[..4], b"\x89PNG");
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (460, 660));
        assert!(crop_png(b"not an image", [0.0, 0.0, 10.0, 10.0], (10.0, 10.0)).is_err());
    }
}
