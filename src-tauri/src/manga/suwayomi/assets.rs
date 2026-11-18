//! What the managed server needs on disk, where it comes from and the
//! SHA-256 each file must have. Nothing here ships with the app: it is
//! downloaded only after the user agrees (see `install`), into
//! `app_data_dir/suwayomi/`, and a file whose checksum is not the one below
//! is deleted, never run.
//!
//! - **Suwayomi-Server** (MPL-2.0, The Suwayomi Project), the release jar,
//!   unmodified: <https://github.com/Suwayomi/Suwayomi-Server> (source of
//!   that tag). Size and checksum taken from the downloaded release file.
//!   Updating it is a pull request that changes these lines, never the app
//!   on its own.
//! - **Eclipse Temurin 21 JRE** (GPL-2.0 with the Classpath Exception), the
//!   Adoptium build for this OS and architecture. Sizes and checksums are the
//!   ones Adoptium publishes (`api.adoptium.net/v3/assets/latest/21/hotspot`,
//!   `binary.package.checksum`), pinned here so a later build cannot replace
//!   them unseen.

use crate::manga::ocr::assets::{Asset, Source};

pub const SERVER_VERSION: &str = "v2.4.2366";

pub const SERVER: Asset = Asset {
    file: "Suwayomi-Server-v2.4.2366.jar",
    size: 182_952_426,
    sha256: "af9feb20af9d7ebe9e30769e6c6ebc7fc7ab1c447388d42e0280b1da5fe07bfd",
    source: Source::Url("https://github.com/Suwayomi/Suwayomi-Server/releases/download/v2.4.2366/Suwayomi-Server-v2.4.2366.jar"),
};

/// A Java runtime for one OS and architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Runtime {
    pub release: &'static str,
    pub archive: Asset,
    /// `.zip` (Windows) or `.tar.gz`.
    pub zip: bool,
    /// Where `java` is inside the archive's top folder.
    pub java: &'static str,
}

#[cfg(test)]
const ADOPTIUM: &str = "https://github.com/adoptium/temurin21-binaries/releases/download";

macro_rules! runtime {
    ($release:literal, $tag:literal, $file:literal, $size:literal, $sha:literal, $zip:literal, $java:literal) => {
        Runtime {
            release: $release,
            archive: Asset { file: $file, size: $size, sha256: $sha, source: Source::Url(concat!("https://github.com/adoptium/temurin21-binaries/releases/download/", $tag, "/", $file)) },
            zip: $zip,
            java: $java,
        }
    };
}

const WIN_X64: Runtime = runtime!(
    "21.0.13+9", "jdk-21.0.13%2B9", "OpenJDK21U-jre_x64_windows_hotspot_21.0.13_9.zip", 49_106_397,
    "2332c0b4cff6bed722da3567c91d5e47568bcba863fa4945d0a8d6fc76158628", true, "bin/java.exe"
);
const WIN_ARM64: Runtime = runtime!(
    "21.0.12.1+1", "jdk-21.0.12.1%2B1", "OpenJDK21U-jre_aarch64_windows_hotspot_21.0.12.1_1.zip", 40_078_274,
    "e82cc17e0bf89a25b0b0ed106d072f2ea420587d0a6870534b71b1dce3ae28c3", true, "bin/java.exe"
);
const LINUX_X64: Runtime = runtime!(
    "21.0.13+9", "jdk-21.0.13%2B9", "OpenJDK21U-jre_x64_linux_hotspot_21.0.13_9.tar.gz", 52_181_297,
    "a535a1df9c6284d9618872984e026ae0352d1f7e9e0605425c0e6114f47351dd", false, "bin/java"
);
const LINUX_ARM64: Runtime = runtime!(
    "21.0.13+9", "jdk-21.0.13%2B9", "OpenJDK21U-jre_aarch64_linux_hotspot_21.0.13_9.tar.gz", 51_266_660,
    "b1734db7ade49c3f6acc6ae06233b8c2dd90e3e2f00f67000ae1c972aaa64d8c", false, "bin/java"
);
const MAC_X64: Runtime = runtime!(
    "21.0.13+9", "jdk-21.0.13%2B9", "OpenJDK21U-jre_x64_mac_hotspot_21.0.13_9.tar.gz", 41_902_098,
    "c909674b1c10c261b9a3fb7e9c0efebd303e33f88a4fe84be7135b5e2d711f89", false, "Contents/Home/bin/java"
);
const MAC_ARM64: Runtime = runtime!(
    "21.0.13+9", "jdk-21.0.13%2B9", "OpenJDK21U-jre_aarch64_mac_hotspot_21.0.13_9.tar.gz", 47_935_740,
    "225bab0a09491ef06835488cd942f5d2befa9a0410a2484f0c8bbcf7506ec09a", false, "Contents/Home/bin/java"
);

/// The runtime for a target of `platform::java_runtime_target`.
pub fn runtime_for(target: &str) -> Option<Runtime> {
    Some(match target {
        "windows-x64" => WIN_X64,
        "windows-aarch64" => WIN_ARM64,
        "linux-x64" => LINUX_X64,
        "linux-aarch64" => LINUX_ARM64,
        "mac-x64" => MAC_X64,
        "mac-aarch64" => MAC_ARM64,
        _ => return None,
    })
}

/// This computer's runtime, if Adoptium builds one for it.
pub fn runtime() -> Option<Runtime> {
    runtime_for(crate::platform::java_runtime_target()?)
}

/// Roughly what an unpacked JRE takes, for the consent dialog (measured on
/// the Linux x64 build: 52 MB packed, 166 MB unpacked).
pub const RUNTIME_UNPACKED_FACTOR: f64 = 3.2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_has_a_pinned_runtime_from_adoptium() {
        for target in ["windows-x64", "windows-aarch64", "linux-x64", "linux-aarch64", "mac-x64", "mac-aarch64"] {
            let runtime = runtime_for(target).unwrap();
            let Source::Url(url) = runtime.archive.source else { panic!("a plain download") };
            assert!(url.starts_with(ADOPTIUM) && url.ends_with(runtime.archive.file), "{url}");
            assert_eq!(runtime.archive.sha256.len(), 64);
            assert_eq!(runtime.zip, target.starts_with("windows"));
        }
        assert!(runtime_for("freebsd-x64").is_none());
        assert_eq!(SERVER.sha256.len(), 64);
        assert!(SERVER.file.contains(SERVER_VERSION));
    }
}
