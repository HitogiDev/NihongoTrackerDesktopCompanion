use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let source = manifest.join("vendor");
    if !source.join("CMakeLists.txt").is_file() {
        panic!(
            "Hoshidicts source is missing at {}. Initialize the submodule with `git submodule update --init --recursive`.",
            source.display()
        );
    }

    let out =
        PathBuf::from(std::env::var("OUT_DIR").expect("cargo output directory")).join("hoshidicts");
    let target = std::env::var("TARGET").expect("rust target");
    let msvc = target.contains("msvc");
    let cmake = tool("CMAKE", "cmake");
    let ninja = tool("NINJA", "ninja");

    let mut configure = msvc_command(&cmake, &target, msvc, true);
    configure
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&out)
        .arg("-G")
        .arg("Ninja")
        .arg(format!("-DCMAKE_MAKE_PROGRAM={}", ninja.display()))
        .arg("-DCMAKE_BUILD_TYPE=Release")
        // Linked into Rust's PIE executables on Linux; without PIC, lld rejects it.
        .arg("-DCMAKE_POSITION_INDEPENDENT_CODE=ON")
        .arg("-DHOSHIDICTS_CLI=OFF")
        .arg("-DHOSHIDICTS_BENCHMARK=OFF");
    if msvc {
        configure.arg("-DCMAKE_CXX_FLAGS=/utf-8 /Zc:__cplusplus");
    }
    // CMake builds for the host by default; a universal macOS bundle runs this
    // once per architecture and each half must link its own copy.
    if target.contains("apple-darwin") {
        let arch = if target.starts_with("aarch64") { "arm64" } else { "x86_64" };
        configure.arg(format!("-DCMAKE_OSX_ARCHITECTURES={arch}"));
    }
    run(configure, "Hoshidicts CMake configure");

    let mut build = msvc_command(&cmake, &target, msvc, false);
    build
        .arg("--build")
        .arg(&out)
        .arg("--target")
        .arg("hoshidicts");
    run(build, "Hoshidicts CMake build");

    for directory in library_directories(&out) {
        println!("cargo:rustc-link-search=native={}", directory.display());
    }
    println!("cargo:rustc-link-lib=static=hoshidicts");
    println!(
        "cargo:rustc-link-lib=static={}",
        if msvc { "zstd_static" } else { "zstd" }
    );
    println!(
        "cargo:rustc-link-lib=static={}",
        if msvc { "deflatestatic" } else { "deflate" }
    );
    // utf8proc only renames its static library under MSVC.
    println!(
        "cargo:rustc-link-lib=static={}",
        if msvc { "utf8proc_static" } else { "utf8proc" }
    );
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if !msvc {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    let mut bridge = cxx_build::bridge("src/lib.rs");
    bridge
        .file(manifest.join("cpp/shim.cc"))
        .include(manifest.join("cpp"))
        .include(source.join("include"))
        .include(source.join("external/utfcpp/source"))
        .include(source.join("external/xxHash"))
        .define("NT_DICT_BRIDGE_HEADER", "\"nihongo-dict/src/lib.rs.h\"");
    if msvc {
        bridge
            .flag("/std:c++latest")
            .flag("/utf-8")
            .flag("/Zc:__cplusplus");
    } else {
        bridge.std("c++23");
    }
    bridge.compile("nihongo-dict-shim");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cpp/shim.cc");
    println!("cargo:rerun-if-changed=cpp/shim.h");
    println!("cargo:rerun-if-changed=vendor/CMakeLists.txt");
}

fn tool(variable: &str, fallback: &str) -> PathBuf {
    std::env::var(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(fallback))
}

fn msvc_command(program: &Path, target: &str, msvc: bool, configure: bool) -> Command {
    let mut command = Command::new(program);
    if msvc {
        if let Some(tool) = cc::windows_registry::find_tool(target, "cl.exe") {
            if configure {
                command.arg(format!("-DCMAKE_C_COMPILER={}", tool.path().display()));
                command.arg(format!("-DCMAKE_CXX_COMPILER={}", tool.path().display()));
                // The linker and librarian from the same toolset as cl.exe. Left
                // to itself CMake takes the first link.exe it finds, and on a
                // machine with two MSVC toolsets that one can belong to the
                // other version, which fails the compiler check.
                for (name, variable) in [("link.exe", "CMAKE_LINKER"), ("lib.exe", "CMAKE_AR")] {
                    let sibling = tool.path().with_file_name(name);
                    if sibling.is_file() {
                        command.arg(format!("-D{variable}={}", sibling.display()));
                    }
                }
                // rc.exe and mt.exe (used when CMake links its test program)
                // from the newest installed Windows SDK that ships both: a
                // partial preview SDK can be picked as the latest and lack them.
                if let Some(bin) = sdk_tools_dir() {
                    command.arg(format!("-DCMAKE_RC_COMPILER={}", bin.join("rc.exe").display()));
                    command.arg(format!("-DCMAKE_MT={}", bin.join("mt.exe").display()));
                }
            }
            for (key, value) in tool.env() {
                command.env(key, value);
            }
        }
    }
    command
}

/// `…\Windows Kits\10\bin\<newest version with rc.exe and mt.exe>\x64`.
fn sdk_tools_dir() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("ProgramFiles(x86)")?).join("Windows Kits\\10\\bin");
    let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(&root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("10.") {
                return None;
            }
            let key = name.split('.').map(|part| part.parse().unwrap_or(0)).collect();
            Some((key, entry.path().join("x64")))
        })
        .filter(|(_, bin)| bin.join("rc.exe").is_file() && bin.join("mt.exe").is_file())
        .collect();
    versions.sort();
    versions.pop().map(|(_, bin)| bin)
}

fn run(mut command: Command, description: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{description} could not start: {error}"));
    if !status.success() {
        panic!("{description} failed with {status}");
    }
}

fn library_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    collect_library_directories(root, &mut directories);
    directories.push(root.to_path_buf());
    directories.sort();
    directories.dedup();
    directories
}

fn collect_library_directories(path: &Path, directories: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            collect_library_directories(&child, directories);
        } else if child.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("lib") || extension.eq_ignore_ascii_case("a")
        }) {
            if let Some(parent) = child.parent() {
                directories.push(parent.to_path_buf());
            }
        }
    }
}
