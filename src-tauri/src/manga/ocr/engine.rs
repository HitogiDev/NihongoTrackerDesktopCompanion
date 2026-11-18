//! The OCR itself: ONNX Runtime running the detector and manga-ocr.
//!
//! 1. **Detect** (`ogkalu/comic-text-and-bubble-detector`, RT-DETR-v2): the
//!    page, resized to 640×640 (bilinear, no aspect kept, as it was trained),
//!    as RGB in `[0, 1]`, plus its real size; the export already turns the
//!    queries into scored boxes in page pixels (`labels`, `boxes`, `scores`).
//! 2. **Recognize** each text box with manga-ocr: the crop goes to grayscale
//!    and back to RGB (`img.convert("L").convert("RGB")`), is resized to
//!    224×224 (bilinear) and normalized as its ViT processor does
//!    (`(x / 255 - 0.5) / 0.5`). The encoder gives 197×768 states; the decoder
//!    is run step by step in a beam search (see `beam`), and the tokens become
//!    text as manga-ocr decodes them (see `text`).
//! 3. **Lay out** the page (see `layout`).
//!
//! ONNX Runtime is loaded from `ocr/` (it is never linked: `ort` is built
//! with `load-dynamic`), after its checksum is verified again. Sessions ask
//! for this platform's accelerator first (`platform::ocr_execution_providers`:
//! DirectML on Windows, CoreML on macOS); if that fails to start, or a page
//! fails to run on it, everything is rebuilt on the CPU and the page retried.
//!
//! An `Engine` is not shared: it lives on the OCR worker's own thread.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use image::{imageops::FilterType, DynamicImage, GenericImageView, RgbImage};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;

use super::assets::{self, Asset, Variant};
use super::layout::{self, Detection};
use super::{beam, text};
use crate::manga::mokuro::MokuroPage;

const DETECTOR_SIZE: u32 = 640;
const RECOGNIZER_SIZE: u32 = 224;

/// Where the sessions run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Accelerated(&'static str),
    Cpu,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Accelerated(name) => name,
            Provider::Cpu => "cpu",
        }
    }
}

pub struct Engine {
    detector: Session,
    encoder: Session,
    decoder: Session,
    vocab: Vec<String>,
    pub provider: Provider,
    dir: PathBuf,
    variant: Variant,
}

/// The SHA-256 of a file, as lowercase hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Loads ONNX Runtime from `dir` once per process (a library cannot be
/// unloaded), after checking that every runtime file is the one pinned.
fn environment(dir: &Path, runtime: &[Asset]) -> Result<&'static Environment, String> {
    static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();
    if let Some(environment) = ENVIRONMENT.get() {
        return Ok(environment);
    }
    for asset in runtime {
        let path = dir.join(asset.file);
        let hash = sha256_file(&path).map_err(|e| format!("{}: {e}", asset.file))?;
        if hash != asset.sha256 {
            return Err(format!("{} does not match its checksum", asset.file));
        }
    }
    crate::platform::ocr_prepare_runtime(dir);
    let library = dir.join(runtime[0].file);
    let environment = ort::init_from(&library)
        .map_err(|e| e.to_string())?
        .with_name("nihongo-tracker-ocr")
        .with_telemetry(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(ENVIRONMENT.get_or_init(|| environment))
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| (n.get() / 2).max(1)).unwrap_or(2)
}

fn session(environment: &Environment, path: &Path, accelerated: bool) -> Result<Session, String> {
    let mut builder = Session::builder(environment).map_err(|e| e.to_string())?;
    if accelerated {
        let providers: Vec<_> = crate::platform::ocr_execution_providers().into_iter().map(|ep| ep.error_on_failure()).collect();
        builder = builder.with_execution_providers(providers).map_err(|e| e.to_string())?;
        // DirectML cannot run memory patterns or in parallel.
        builder = builder.with_memory_pattern(false).map_err(|e| e.to_string())?;
        builder = builder.with_parallel_execution(false).map_err(|e| e.to_string())?;
    } else {
        builder = builder.with_intra_threads(threads()).map_err(|e| e.to_string())?;
    }
    builder.commit_from_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

impl Engine {
    /// The runtime and a variant's models from `dir`, on the accelerator if
    /// it starts, on the CPU otherwise.
    pub fn load(dir: &Path, variant: Variant) -> Result<Engine, String> {
        let runtime = assets::runtime().ok_or("unsupported")?;
        let models = assets::models(variant);
        if let Some(missing) = runtime.iter().chain(&models).find(|asset| !asset.present(dir)) {
            return Err(format!("missing {}", missing.file));
        }
        let environment = environment(dir, runtime)?;
        let vocab = text::parse_vocab(&std::fs::read_to_string(dir.join(assets::VOCAB.file)).map_err(|e| e.to_string())?);
        let accelerator = crate::platform::ocr_accelerator();
        if let Some(name) = accelerator {
            match Self::sessions(environment, dir, variant, true) {
                Ok((detector, encoder, decoder)) => {
                    return Ok(Engine { detector, encoder, decoder, vocab, provider: Provider::Accelerated(name), dir: dir.to_path_buf(), variant });
                }
                Err(error) => log::warn!("OCR: {name} did not start ({error}); using the CPU"),
            }
        }
        let (detector, encoder, decoder) = Self::sessions(environment, dir, variant, false)?;
        Ok(Engine { detector, encoder, decoder, vocab, provider: Provider::Cpu, dir: dir.to_path_buf(), variant })
    }

    fn sessions(environment: &Environment, dir: &Path, variant: Variant, accelerated: bool) -> Result<(Session, Session, Session), String> {
        let [encoder, decoder] = assets::recognizer(variant);
        Ok((
            session(environment, &dir.join(assets::DETECTOR.file), accelerated)?,
            session(environment, &dir.join(encoder.file), accelerated)?,
            session(environment, &dir.join(decoder.file), accelerated)?,
        ))
    }

    /// Something failed on the accelerator: the rest runs on the CPU.
    fn fall_back(&mut self, error: &str) -> Result<(), String> {
        log::warn!("OCR: {} failed ({error}); moving to the CPU", self.provider.name());
        let environment = environment(&self.dir, assets::runtime().ok_or("unsupported")?)?;
        let (detector, encoder, decoder) = Self::sessions(environment, &self.dir, self.variant, false)?;
        *self = Engine { detector, encoder, decoder, vocab: std::mem::take(&mut self.vocab), provider: Provider::Cpu, dir: self.dir.clone(), variant: self.variant };
        Ok(())
    }

    /// Runs `f`, and on the accelerator, once more on the CPU if it fails.
    fn with_fallback<T>(&mut self, mut f: impl FnMut(&mut Engine) -> Result<T, String>) -> Result<T, String> {
        match f(self) {
            Err(error) if self.provider != Provider::Cpu => {
                self.fall_back(&error)?;
                f(self)
            }
            other => other,
        }
    }

    /// The text of a page, as a `.mokuro` page.
    pub fn recognize_page(&mut self, image: &DynamicImage, img_path: &str) -> Result<MokuroPage, String> {
        self.with_fallback(|engine| {
            let size = image.dimensions();
            let detections = engine.detect(image)?;
            let boxes = layout::text_boxes(&detections, size);
            let order = layout::reading_order(&boxes);
            let mut read = Vec::with_capacity(order.len());
            for index in order {
                let bbox = boxes[index];
                read.push((bbox, engine.recognize(image, bbox)?));
            }
            Ok(layout::page(img_path, size, read))
        })
    }

    /// The text inside one box of a page (a box the user drew).
    pub fn recognize_box(&mut self, image: &DynamicImage, bbox: [f64; 4]) -> Result<String, String> {
        self.with_fallback(|engine| engine.recognize(image, bbox))
    }

    fn detect(&mut self, image: &DynamicImage) -> Result<Vec<Detection>, String> {
        let (width, height) = image.dimensions();
        let rgb = image.resize_exact(DETECTOR_SIZE, DETECTOR_SIZE, FilterType::Triangle).to_rgb8();
        let pixels = chw(&rgb, |value| f32::from(value) / 255.0);
        let side = DETECTOR_SIZE as usize;
        let images = Tensor::from_array(([1usize, 3, side, side], pixels)).map_err(|e| e.to_string())?;
        let sizes = Tensor::from_array(([1usize, 2], vec![i64::from(width), i64::from(height)])).map_err(|e| e.to_string())?;
        let outputs = self.detector.run(ort::inputs!["images" => images, "orig_target_sizes" => sizes]).map_err(|e| e.to_string())?;
        let (_, labels) = outputs["labels"].try_extract_tensor::<i64>().map_err(|e| e.to_string())?;
        let (_, boxes) = outputs["boxes"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (_, scores) = outputs["scores"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        Ok(labels
            .iter()
            .zip(scores)
            .zip(boxes.chunks_exact(4))
            .map(|((&label, &score), b)| Detection { label, score, bbox: [f64::from(b[0]), f64::from(b[1]), f64::from(b[2]), f64::from(b[3])] })
            .collect())
    }

    fn recognize(&mut self, image: &DynamicImage, bbox: [f64; 4]) -> Result<String, String> {
        let pixels = recognizer_input(image, bbox).ok_or("empty box")?;
        let side = RECOGNIZER_SIZE as usize;
        let input = Tensor::from_array(([1usize, 3, side, side], pixels)).map_err(|e| e.to_string())?;
        let outputs = self.encoder.run(ort::inputs!["pixel_values" => input]).map_err(|e| e.to_string())?;
        let (shape, states) = outputs["last_hidden_state"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (tokens, hidden) = (shape[1] as usize, shape[2] as usize);
        let states = states.to_vec();
        drop(outputs);

        let decoder = &mut self.decoder;
        let ids = beam::search(&beam::Config::manga_ocr(), |beams: &[Vec<i64>]| -> Result<Vec<Vec<f32>>, String> {
            let batch = beams.len();
            let length = beams[0].len();
            let input_ids: Vec<i64> = beams.iter().flatten().copied().collect();
            let mut repeated = Vec::with_capacity(batch * states.len());
            for _ in 0..batch {
                repeated.extend_from_slice(&states);
            }
            let input_ids = Tensor::from_array(([batch, length], input_ids)).map_err(|e| e.to_string())?;
            let hidden_states = Tensor::from_array(([batch, tokens, hidden], repeated)).map_err(|e| e.to_string())?;
            let outputs = decoder
                .run(ort::inputs!["input_ids" => input_ids, "encoder_hidden_states" => hidden_states])
                .map_err(|e| e.to_string())?;
            let (shape, logits) = outputs["logits"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            let vocab = shape[2] as usize;
            Ok((0..batch)
                .map(|beam| {
                    let last = (beam * length + length - 1) * vocab;
                    logits[last..last + vocab].to_vec()
                })
                .collect())
        })?;
        Ok(text::decode(&ids, &self.vocab))
    }
}

/// Channels first, as the models take them.
fn chw(rgb: &RgbImage, value: impl Fn(u8) -> f32) -> Vec<f32> {
    let (w, h) = rgb.dimensions();
    let plane = (w * h) as usize;
    let mut out = vec![0f32; plane * 3];
    for (i, pixel) in rgb.pixels().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = value(pixel[c]);
        }
    }
    out
}

/// A box of the page as manga-ocr's encoder takes it, or `None` when it has
/// no pixels inside the page.
pub fn recognizer_input(image: &DynamicImage, bbox: [f64; 4]) -> Option<Vec<f32>> {
    let (width, height) = image.dimensions();
    let x1 = bbox[0].max(0.0).floor() as u32;
    let y1 = bbox[1].max(0.0).floor() as u32;
    let x2 = (bbox[2].ceil().max(0.0) as u32).min(width);
    let y2 = (bbox[3].ceil().max(0.0) as u32).min(height);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    let crop = image.crop_imm(x1, y1, x2 - x1, y2 - y1);
    let gray = DynamicImage::ImageLuma8(crop.to_luma8());
    let rgb = gray.resize_exact(RECOGNIZER_SIZE, RECOGNIZER_SIZE, FilterType::Triangle).to_rgb8();
    Some(chw(&rgb, |value| (f32::from(value) / 255.0 - 0.5) / 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_are_normalized_like_vit() {
        let mut page = RgbImage::from_pixel(100, 100, image::Rgb([255, 255, 255]));
        for x in 0..50 {
            for y in 0..100 {
                page.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        let image = DynamicImage::ImageRgb8(page);
        let input = recognizer_input(&image, [0.0, 0.0, 100.0, 100.0]).unwrap();
        let side = RECOGNIZER_SIZE as usize;
        assert_eq!(input.len(), 3 * side * side);
        // Black is -1, white is 1, and the three channels are the same gray.
        assert!((input[0] + 1.0).abs() < 1e-6);
        assert!((input[side - 1] - 1.0).abs() < 1e-6);
        assert_eq!(input[10], input[side * side + 10]);
        assert!(recognizer_input(&image, [120.0, 0.0, 200.0, 50.0]).is_none());
    }

    #[test]
    fn checksums_are_hex_sha256() {
        let path = std::env::temp_dir().join(format!("ntdc-sha-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(path);
    }

    /// The whole pipeline on a real page, with the real runtime and models.
    /// Needs `NTDC_OCR_DIR` (a folder with the files of `assets::all`) and
    /// `NTDC_OCR_PAGE` (an image); prints what it read.
    #[test]
    #[ignore]
    fn reads_a_real_page() {
        let dir = PathBuf::from(std::env::var("NTDC_OCR_DIR").expect("NTDC_OCR_DIR"));
        let page = std::env::var("NTDC_OCR_PAGE").expect("NTDC_OCR_PAGE");
        let variant = if std::env::var("NTDC_OCR_VARIANT").as_deref() == Ok("fp16") { Variant::Fp16 } else { Variant::Int8 };
        let started = std::time::Instant::now();
        let mut engine = Engine::load(&dir, variant).unwrap();
        let loaded = started.elapsed();
        let image = image::open(&page).unwrap();
        let started = std::time::Instant::now();
        let result = engine.recognize_page(&image, "page.jpg").unwrap();
        let elapsed = started.elapsed();
        println!("provider: {}, load {:?}, page {:?}", engine.provider.name(), loaded, elapsed);
        for block in &result.blocks {
            println!("{:?} vertical={} lines={:?}", block.bbox.map(|v| v.round()), block.vertical, block.lines);
        }
        assert!(!result.blocks.is_empty());
        if let Ok(out) = std::env::var("NTDC_OCR_OUT") {
            std::fs::write(out, serde_json::to_string_pretty(&result).unwrap()).unwrap();
        }
        if let Ok(expected) = std::env::var("NTDC_OCR_EXPECT") {
            let read: Vec<String> = result.blocks.iter().map(|block| block.lines.concat()).collect();
            assert_eq!(read.join("|"), expected);
        }
    }
}
