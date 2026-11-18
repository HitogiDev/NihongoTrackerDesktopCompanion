//! What the built-in OCR needs on disk, where each file comes from and the
//! SHA-256 it must have. Everything lives in `app_data_dir/ocr/`.
//!
//! Nothing here ships with the app: the files are downloaded only when the
//! user agrees to (see `download`), and a file whose checksum is not the one
//! written below is never kept. The checksums were taken from the files
//! themselves, downloaded from the URLs below (which point at fixed commits
//! and immutable package versions, so they cannot change under us).
//!
//! - **Detector**: `ogkalu/comic-text-and-bubble-detector` (Apache-2.0), the
//!   int8 RT-DETR-v2 export. Classes: 0 bubble, 1 text in a bubble, 2 text
//!   outside bubbles.
//! - **Recognizer**: `onnx-community/manga-ocr-base-ONNX` (Apache-2.0), the
//!   ONNX conversion of kha-white/manga-ocr-base. Its vocabulary is not in that
//!   repository, so `vocab.txt` comes from kha-white/manga-ocr-base itself.
//!   The fp16 decoder of the conversion does not load (ONNX Runtime 1.29
//!   rejects it: a Cast typed float16
//!   feeding a float32 input), so the "precise" variant pairs the fp16
//!   encoder with the full-precision decoder.
//! - **meikiocr** (rtr46/meikiocr, the "fast" built-in engine): the models
//!   its pipeline uses, `rtr46/meiki.text.detect.v0` (the v0.1 960×544 line
//!   detector) and `rtr46/meiki.txt.recognition.v0` (the horizontal 960×32
//!   and vertical 32×480 recognizers), both LGPL-3.0. Unmodified files,
//!   fetched from the author's repositories at pinned commits only when the
//!   user asks for them; the app is not built from them (see `meiki`).
//! - **ONNX Runtime** (MIT) 1.24.4, the official build Microsoft publishes on
//!   nuget.org: `Microsoft.ML.OnnxRuntime` for Linux and macOS (the macOS
//!   build has CoreML), `Microsoft.ML.OnnxRuntime.DirectML` for Windows plus
//!   the `DirectML.dll` it needs from `Microsoft.AI.DirectML` 1.15.4. A
//!   package holds every platform, so only the library this computer needs
//!   is read out of it, by HTTP ranges (`Source::ZipEntry`).
//! - **Intel Macs** get ONNX Runtime **1.23.2** instead: 1.24 dropped the
//!   `osx-x64` build (its package has `osx-arm64` only), and 1.23.2 is the
//!   last release that has it. That library needs macOS 13.4 or later. It
//!   implements the C API up to version 23, which is why `ort` is built for
//!   `api-23` (Cargo.toml): a newer runtime serves an older API, never the
//!   other way round. 1.23 has no CPU kernel for the `ConvInteger` of the
//!   int8 encoder (the session does not even build), so there the int8
//!   variant pairs the fp16 encoder with the int8 decoder (see
//!   `recognizer_for`); the detector and both decoders load as they are.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The ONNX Runtime version downloaded everywhere but on Intel Macs.
pub const RUNTIME_VERSION: &str = "1.24.4";
/// The last ONNX Runtime with a build for Intel Macs (`osx-x64`). `ort` is
/// built for its API (`api-23` in Cargo.toml): a runtime older than the API
/// `ort` asks for would refuse to load.
pub const RUNTIME_VERSION_OSX_X64: &str = "1.23.2";

#[cfg(test)]
const DETECTOR_REV: &str = "16e8a622f91fabc6b5b65c96d32d1183f8843546";
#[cfg(test)]
const OCR_ONNX_REV: &str = "f9023406bb2f6b17df67bc4a327c56ecd20611f0";
#[cfg(test)]
const OCR_BASE_REV: &str = "aa6573bd10b0d446cbf622e29c3e084914df9741";
#[cfg(test)]
const MEIKI_DETECT_REV: &str = "a9cffa4f60cbf72ddb87edf19c6f98a01cd042e6";
#[cfg(test)]
const MEIKI_REC_REV: &str = "a28cf5874dc2438ebb1c86336be26bcec51e3375";

const NUGET_ORT: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.24.4/microsoft.ml.onnxruntime.1.24.4.nupkg";
const NUGET_ORT_DML: &str =
    "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.4/microsoft.ml.onnxruntime.directml.1.24.4.nupkg";
const NUGET_ORT_1_23: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.23.2/microsoft.ml.onnxruntime.1.23.2.nupkg";
const NUGET_DIRECTML: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg";

/// Where a file is downloaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The file itself.
    Url(&'static str),
    /// A deflated entry of a zip (a NuGet package) on a server that answers
    /// ranges: its local header is at `header_offset` and its data is
    /// `compressed` bytes long. Only those bytes are downloaded.
    ZipEntry { url: &'static str, entry: &'static str, header_offset: u64, compressed: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asset {
    /// The file's name in `ocr/`.
    pub file: &'static str,
    /// Its size once downloaded (and inflated).
    pub size: u64,
    pub sha256: &'static str,
    pub source: Source,
}

impl Asset {
    /// The bytes that travel: the compressed entry of a package.
    pub fn transfer_size(&self) -> u64 {
        match self.source {
            Source::Url(_) => self.size,
            Source::ZipEntry { compressed, .. } => compressed,
        }
    }

    /// On disk with the size it should have. The checksum was verified when
    /// it was downloaded; the runtime's library is verified again before it
    /// is loaded (see `engine`).
    pub fn present(&self, dir: &Path) -> bool {
        std::fs::metadata(dir.join(self.file)).is_ok_and(|meta| meta.is_file() && meta.len() == self.size)
    }
}

/// Which recognizer weights: the default int8 pair, or a larger, slightly
/// more faithful one.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "OcrVariant"))]
pub enum Variant {
    #[default]
    Int8,
    Fp16,
}

macro_rules! hf {
    ($repo:literal, $rev:literal, $path:literal) => {
        Source::Url(concat!("https://huggingface.co/", $repo, "/resolve/", $rev, "/", $path))
    };
}

// `concat!` needs literals: the revisions are repeated in the URLs below and
// checked against the constants by a test.
pub const DETECTOR: Asset = Asset {
    file: "detector_int8.onnx",
    size: 43_838_857,
    sha256: "b5022ad46416b6fe4f88b0cc082cfd2ff5b1cfc624088c2f19879485493f5913",
    source: hf!("ogkalu/comic-text-and-bubble-detector", "16e8a622f91fabc6b5b65c96d32d1183f8843546", "detector_int8.onnx"),
};

pub const VOCAB: Asset = Asset {
    file: "vocab.txt",
    size: 24_072,
    sha256: "344fbb6b8bf18c57839e924e2c9365434697e0227fac00b88bb4899b78aa594d",
    source: hf!("kha-white/manga-ocr-base", "aa6573bd10b0d446cbf622e29c3e084914df9741", "vocab.txt"),
};

const ENCODER_INT8: Asset = Asset {
    file: "encoder_model_int8.onnx",
    size: 86_967_767,
    sha256: "ddd1af56963093795705fa38da6ce7e6567d1658e7c7359db7e13fcd37dbf279",
    source: hf!("onnx-community/manga-ocr-base-ONNX", "f9023406bb2f6b17df67bc4a327c56ecd20611f0", "onnx/encoder_model_int8.onnx"),
};

const DECODER_INT8: Asset = Asset {
    file: "decoder_model_int8.onnx",
    size: 29_627_936,
    sha256: "2e7177d2b0a59f1c612b694ed70c13971bee765cc2b2bc7bc9376e4753652f27",
    source: hf!("onnx-community/manga-ocr-base-ONNX", "f9023406bb2f6b17df67bc4a327c56ecd20611f0", "onnx/decoder_model_int8.onnx"),
};

const ENCODER_FP16: Asset = Asset {
    file: "encoder_model_fp16.onnx",
    size: 171_851_891,
    sha256: "1a6a57bc3608195c4577b13ac3aadab810dce42fa22c5a3acf0570bffc013b60",
    source: hf!("onnx-community/manga-ocr-base-ONNX", "f9023406bb2f6b17df67bc4a327c56ecd20611f0", "onnx/encoder_model_fp16.onnx"),
};

const DECODER_FP32: Asset = Asset {
    file: "decoder_model.onnx",
    size: 117_445_718,
    sha256: "31ca14d6dee6b3966144e128d0481d5a91f5083cbced81fe7a9571713fa50cd4",
    source: hf!("onnx-community/manga-ocr-base-ONNX", "f9023406bb2f6b17df67bc4a327c56ecd20611f0", "onnx/decoder_model.onnx"),
};

/// meikiocr's line detector (D-FINE, at most 64 lines a page).
pub const MEIKI_DETECTOR: Asset = Asset {
    file: "meiki.text.detect.v0.1.960x544.onnx",
    size: 14_503_825,
    sha256: "40b6a016667745cae7d3055929ae3b8b1e7716aac795f5904cd3c2c7c3b8404b",
    source: hf!("rtr46/meiki.text.detect.v0", "a9cffa4f60cbf72ddb87edf19c6f98a01cd042e6", "meiki.text.detect.v0.1.960x544.onnx"),
};

/// meikiocr's recognizer for horizontal lines (48 characters at most).
pub const MEIKI_HORIZONTAL: Asset = Asset {
    file: "meiki.text.rec.v0.960x32.onnx",
    size: 18_593_254,
    sha256: "3e96bc772fbee9717e536a6353032bb944c3382dd2f6960ef4890decda43b000",
    source: hf!("rtr46/meiki.txt.recognition.v0", "a28cf5874dc2438ebb1c86336be26bcec51e3375", "meiki.text.rec.v0.960x32.onnx"),
};

/// meikiocr's recognizer for vertical lines (24 characters a segment).
pub const MEIKI_VERTICAL: Asset = Asset {
    file: "meiki.text.rec.v0.vertical.32x480.onnx",
    size: 12_872_961,
    sha256: "2c2a83a23bc3b7e6c63962175f507ecc6c5e85cc174f17bdec37d9bbd0bf895a",
    source: hf!("rtr46/meiki.txt.recognition.v0", "a28cf5874dc2438ebb1c86336be26bcec51e3375", "meiki.text.rec.v0.vertical.32x480.onnx"),
};

/// meikiocr's models, in the order they are downloaded.
pub fn meiki_models() -> Vec<Asset> {
    vec![MEIKI_DETECTOR, MEIKI_HORIZONTAL, MEIKI_VERTICAL]
}

/// The encoder and decoder of a variant for a runtime target. ONNX Runtime
/// 1.23 (Intel Macs) cannot run the int8 encoder: its int8 variant takes the
/// fp16 one, with the int8 decoder.
pub fn recognizer_for(target: Option<&str>, variant: Variant) -> [Asset; 2] {
    match variant {
        Variant::Int8 if target == Some("osx-x64") => [ENCODER_FP16, DECODER_INT8],
        Variant::Int8 => [ENCODER_INT8, DECODER_INT8],
        Variant::Fp16 => [ENCODER_FP16, DECODER_FP32],
    }
}

/// The encoder and decoder of a variant on this computer.
pub fn recognizer(variant: Variant) -> [Asset; 2] {
    recognizer_for(crate::platform::ocr_runtime_target(), variant)
}

/// The models a variant needs, in the order they are downloaded.
pub fn models(variant: Variant) -> Vec<Asset> {
    let [encoder, decoder] = recognizer(variant);
    vec![DETECTOR, encoder, decoder, VOCAB]
}

const fn entry(file: &'static str, size: u64, sha256: &'static str, url: &'static str, entry: &'static str, header_offset: u64, compressed: u64) -> Asset {
    Asset { file, size, sha256, source: Source::ZipEntry { url, entry, header_offset, compressed } }
}

const WIN_X64: &[Asset] = &[
    entry(
        "onnxruntime.dll", 17_328_152, "e7eedec6a6f26dc39dc948276a75ef6d2bee3fff944d874ceed0bbd3b97bff40",
        NUGET_ORT_DML, "runtimes/win-x64/native/onnxruntime.dll", 6_342_473, 6_039_111,
    ),
    entry(
        "DirectML.dll", 18_527_776, "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1",
        NUGET_DIRECTML, "bin/x64-win/DirectML.dll", 142_873_380, 9_332_741,
    ),
];
const WIN_ARM64: &[Asset] = &[
    entry(
        "onnxruntime.dll", 17_546_784, "939fa0be435d253eadd9e394ee94ad6b6ac1878c83b64118bb4bda669eb0e28c",
        NUGET_ORT_DML, "runtimes/win-arm64/native/onnxruntime.dll", 421_215, 5_908_897,
    ),
    entry(
        "DirectML.dll", 18_444_832, "77b0db83ff903f2323f5caf538499d75af6038bbea23b7959f7d232d9a4ab9d4",
        NUGET_DIRECTML, "bin/arm64-win/DirectML.dll", 1_412, 9_202_115,
    ),
];
const LINUX_X64: &[Asset] = &[entry(
    "libonnxruntime.so", 22_159_232, "d132535d051344ff5c64c9c200004150559049a81ed330eb4422c1962fb6b7e4",
    NUGET_ORT, "runtimes/linux-x64/native/libonnxruntime.so", 96_729_937, 7_950_245,
)];
const LINUX_ARM64: &[Asset] = &[entry(
    "libonnxruntime.so", 18_625_376, "52b2a0e75e79468404284fec38ad5ee1a7a996232274f5e1b84f3e793fb07554",
    NUGET_ORT, "runtimes/linux-arm64/native/libonnxruntime.so", 89_750_573, 6_977_175,
)];
const MACOS_ARM64: &[Asset] = &[entry(
    "libonnxruntime.dylib", 35_451_632, "0db33e160224e480c17932799d1dfaea59853b8e722b57bf61932c87e03586f0",
    NUGET_ORT, "runtimes/osx-arm64/native/libonnxruntime.dylib", 104_682_025, 9_742_969,
)];
/// 1.23.2: see `RUNTIME_VERSION_OSX_X64`. The same file name as on Apple
/// silicon: one computer only ever has one of them.
const MACOS_X64: &[Asset] = &[entry(
    "libonnxruntime.dylib", 39_742_608, "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054",
    NUGET_ORT_1_23, "runtimes/osx-x64/native/libonnxruntime.dylib", 100_752_058, 11_498_106,
)];

/// The runtime files for a target (`platform::ocr_runtime_target`). The
/// first one is the library `ort` loads; the rest are what it needs beside it.
pub fn runtime_for(target: &str) -> Option<&'static [Asset]> {
    match target {
        "win-x64" => Some(WIN_X64),
        "win-arm64" => Some(WIN_ARM64),
        "linux-x64" => Some(LINUX_X64),
        "linux-arm64" => Some(LINUX_ARM64),
        "osx-arm64" => Some(MACOS_ARM64),
        "osx-x64" => Some(MACOS_X64),
        _ => None,
    }
}

/// The ONNX Runtime version of a target's files.
pub fn runtime_version_for(target: &str) -> &'static str {
    if target == "osx-x64" { RUNTIME_VERSION_OSX_X64 } else { RUNTIME_VERSION }
}

/// This computer's runtime files. `None` where no build exists (32-bit
/// systems, other operating systems).
pub fn runtime() -> Option<&'static [Asset]> {
    crate::platform::ocr_runtime_target().and_then(runtime_for)
}

/// The ONNX Runtime version this computer downloads.
pub fn runtime_version() -> &'static str {
    crate::platform::ocr_runtime_target().map_or(RUNTIME_VERSION, runtime_version_for)
}

/// Which models a download is for: manga-ocr in a variant, or meikiocr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Models {
    MangaOcr(Variant),
    Meiki,
}

impl Models {
    pub fn assets(self) -> Vec<Asset> {
        match self {
            Models::MangaOcr(variant) => models(variant),
            Models::Meiki => meiki_models(),
        }
    }

    /// Every file of these models in any variant, for "delete".
    pub fn files(self) -> Vec<&'static str> {
        match self {
            Models::MangaOcr(_) => [DETECTOR, VOCAB, ENCODER_INT8, DECODER_INT8, ENCODER_FP16, DECODER_FP32].iter().map(|asset| asset.file).collect(),
            Models::Meiki => meiki_models().iter().map(|asset| asset.file).collect(),
        }
    }
}

/// Everything some models need here, runtime first.
pub fn all_for(models: Models) -> Option<Vec<Asset>> {
    let mut assets = runtime()?.to_vec();
    assets.extend(models.assets());
    Some(assets)
}

/// The runtime's files of every target, for "delete".
pub fn runtime_files() -> Vec<&'static str> {
    let mut files: Vec<&'static str> = [WIN_X64, WIN_ARM64, LINUX_X64, LINUX_ARM64, MACOS_ARM64, MACOS_X64]
        .iter()
        .flat_map(|assets| assets.iter().map(|asset| asset.file))
        .collect();
    files.sort_unstable();
    files.dedup();
    files
}

/// Every file name the OCR may have written, for "delete models".
pub fn known_files() -> Vec<&'static str> {
    let mut files: Vec<&'static str> = runtime_files()
        .into_iter()
        .chain(Models::MangaOcr(Variant::Int8).files())
        .chain(Models::Meiki.files())
        .collect();
    files.sort_unstable();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_point_at_the_pinned_revisions() {
        for asset in models(Variant::Int8).into_iter().chain(models(Variant::Fp16)).chain(meiki_models()) {
            let Source::Url(url) = asset.source else { panic!("models are plain files") };
            assert!(url.starts_with("https://huggingface.co/"), "{url}");
            assert_eq!(asset.sha256.len(), 64);
            assert!(
                [DETECTOR_REV, OCR_ONNX_REV, OCR_BASE_REV, MEIKI_DETECT_REV, MEIKI_REC_REV].iter().any(|rev| url.contains(&format!("/resolve/{rev}/"))),
                "{url}"
            );
            assert!(url.ends_with(asset.file), "{url} is saved as {}", asset.file);
        }
    }

    #[test]
    fn every_target_has_a_library_from_an_official_package() {
        for target in ["win-x64", "win-arm64", "linux-x64", "linux-arm64", "osx-arm64", "osx-x64"] {
            let assets = runtime_for(target).unwrap();
            assert!(assets[0].file.contains("onnxruntime"));
            let version = runtime_version_for(target);
            for asset in assets {
                assert_eq!(asset.sha256.len(), 64);
                assert!(asset.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
                let Source::ZipEntry { url, entry, compressed, .. } = asset.source else { panic!("from a package") };
                assert!(url.starts_with("https://api.nuget.org/v3-flatcontainer/microsoft."), "{url}");
                assert!(url.contains(&format!("/{version}/")) || url.contains("microsoft.ai.directml/1.15.4/"), "{url}");
                assert!(entry.ends_with(asset.file));
                assert!(entry.contains(&format!("/{target}/")) || entry.contains("-win/"), "{entry} is not for {target}");
                assert!(compressed < asset.size);
            }
        }
        assert!(runtime_for("osx-x86").is_none());
    }

    #[test]
    fn intel_macs_get_the_last_runtime_built_for_them() {
        let assets = runtime_for("osx-x64").expect("Intel Macs are supported");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].file, "libonnxruntime.dylib");
        assert_eq!(assets[0].sha256, "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054");
        assert_eq!(assets[0].size, 39_742_608);
        let Source::ZipEntry { url, .. } = assets[0].source else { panic!("from a package") };
        assert!(url.contains("microsoft.ml.onnxruntime/1.23.2/"), "{url}");
        assert_eq!(runtime_version_for("osx-x64"), "1.23.2");
        assert_eq!(runtime_version_for("osx-arm64"), RUNTIME_VERSION);
        // `ort` asks for the C API of the oldest runtime downloaded.
        assert_eq!(ort::MINOR_VERSION, 23);
        // 1.23 cannot build the int8 encoder: the fp16 one goes instead.
        assert_eq!(recognizer_for(Some("osx-x64"), Variant::Int8), [ENCODER_FP16, DECODER_INT8]);
        assert_eq!(recognizer_for(Some("osx-x64"), Variant::Fp16), [ENCODER_FP16, DECODER_FP32]);
        assert_eq!(recognizer_for(Some("osx-arm64"), Variant::Int8), [ENCODER_INT8, DECODER_INT8]);
        assert_eq!(recognizer_for(Some("linux-x64"), Variant::Int8), [ENCODER_INT8, DECODER_INT8]);
    }

    #[test]
    fn meikiocr_and_manga_ocr_install_apart() {
        let meiki: Vec<&str> = Models::Meiki.assets().iter().map(|asset| asset.file).collect();
        assert_eq!(meiki, ["meiki.text.detect.v0.1.960x544.onnx", "meiki.text.rec.v0.960x32.onnx", "meiki.text.rec.v0.vertical.32x480.onnx"]);
        let manga = Models::MangaOcr(Variant::Int8).files();
        assert!(meiki.iter().all(|file| !manga.contains(file)), "no file is shared but the runtime");
        assert!(Models::Meiki.assets().iter().all(|asset| matches!(asset.source, Source::Url(url) if url.contains("/rtr46/meiki."))));
        for file in Models::Meiki.files().into_iter().chain(manga).chain(runtime_files()) {
            assert!(known_files().contains(&file), "{file}");
        }
    }

    #[test]
    fn the_names_never_collide_by_case() {
        let mut lower: Vec<String> = known_files().iter().map(|file| file.to_lowercase()).collect();
        let count = lower.len();
        lower.sort();
        lower.dedup();
        assert_eq!(lower.len(), count);
    }
}
