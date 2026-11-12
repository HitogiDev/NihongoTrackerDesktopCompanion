use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("vendor");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("hoshidicts");

    if !source.join("CMakeLists.txt").exists() {
        panic!(
            "crates/nihongo-dict/vendor is empty — run:\n  \
             git submodule update --init --recursive --depth 1"
        );
    }

    let target = std::env::var("TARGET").unwrap();
    let msvc = target.contains("msvc");

    if msvc {
        apply_msvc_patch(&source, &manifest);
    }
    build_engine(&source, &out, &target, msvc);
    link_engine(&out, msvc);
    build_shim(&manifest, &source, msvc);

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cpp/shim.cc");
    println!("cargo:rerun-if-changed=cpp/shim.h");
    println!("cargo:rerun-if-changed=patches");
}

/// MSVC caps a string literal at 16,380 bytes (C2026) and the deconjugation
/// rules are a single 35 KB raw literal, so the header has to be split into
/// adjacent literals before it will compile at all.
///
/// Applied here rather than left as a manual step, so a clean checkout builds
/// with nothing but `cargo build`. Idempotent: a tree that already has the patch
/// is left alone, which also covers the day upstream merges it.
fn apply_msvc_patch(source: &Path, manifest: &Path) {
    let header = source.join("src/deconjugator/deconjugation_rules.hpp");
    let Ok(contents) = std::fs::read_to_string(&header) else {
        panic!("cannot read {}", header.display());
    };
    if contents.matches("R\"json(").count() > 1 {
        return; // already split, by this build or by upstream
    }

    let patch = manifest.join("patches/0001-msvc-split-string-literal.patch");
    let applied = Command::new("git")
        .current_dir(source)
        .args(["apply", "--whitespace=nowarn"])
        .arg(&patch)
        .status();

    let ok = matches!(applied, Ok(status) if status.success());
    if !ok {
        // No git, or the vendored file moved on: split it in place instead. The
        // build must not depend on a working `git apply`.
        split_rules_literal(&header);
    }
}

/// The same split the patch performs, done directly on the file. Cutting at line
/// boundaries keeps the embedded JSON readable.
fn split_rules_literal(header: &Path) {
    const OPEN: &str = "R\"json(";
    const CLOSE: &str = ")json\"";
    const CHUNK: usize = 8000;

    let contents = std::fs::read_to_string(header).expect("read rules header");
    let contents = contents.replace("\r\n", "\n");
    let (Some(start), Some(end)) = (contents.find(OPEN), contents.rfind(CLOSE)) else {
        panic!("the deconjugation rules literal is not where it was expected");
    };

    let body = &contents[start + OPEN.len()..end];
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in body.split('\n') {
        if !current.is_empty() && current.len() + line.len() + 1 > CHUNK {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    let joined = chunks
        .iter()
        .map(|chunk| format!("{OPEN}{chunk}{CLOSE}"))
        .collect::<Vec<_>>()
        .join("\n");
    let patched = format!("{}{joined}{}", &contents[..start], &contents[end + CLOSE.len()..]);
    std::fs::write(header, patched).expect("write rules header");
}

/// CMake is driven by hand rather than through the `cmake` crate because the
/// generator and the compiler both have to be pinned: an MSYS2 `cmake` on PATH
/// would happily pick its own compiler and produce MinGW objects that cannot
/// link into an MSVC binary.
fn build_engine(source: &Path, out: &Path, target: &str, msvc: bool) {
    let cmake = tool("CMAKE", "cmake");
    let ninja = tool("NINJA", "ninja");

    let mut configure = command(&cmake, target, msvc, true);
    configure
        .arg("-S")
        .arg(source)
        .arg("-B")
        .arg(out)
        .arg("-G")
        .arg("Ninja")
        .arg(format!("-DCMAKE_MAKE_PROGRAM={}", ninja.display()))
        .arg("-DCMAKE_BUILD_TYPE=Release");

    if msvc {
        // Both mandatory, neither cosmetic:
        //  /utf-8          the rules are a UTF-8 raw literal; without it MSVC
        //                  decodes the source with the system code page.
        //  /Zc:__cplusplus otherwise MSVC claims __cplusplus == 199711L and
        //                  utfcpp hides the C++17 API the library calls.
        configure.arg("-DCMAKE_CXX_FLAGS=/utf-8 /Zc:__cplusplus");
    }
    run(configure, "cmake configure");

    let mut build = command(&cmake, target, msvc, false);
    build.arg("--build").arg(out).arg("--target").arg("hoshidicts");
    run(build, "cmake build");
}

fn link_engine(out: &Path, msvc: bool) {
    for dir in [
        out.to_path_buf(),
        out.join("external/zstd/build/cmake/lib"),
        out.join("external/libdeflate"),
    ] {
        println!("cargo:rustc-link-search=native={}", dir.display());
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
    if !msvc {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
}

fn build_shim(manifest: &Path, source: &Path, msvc: bool) {
    let mut bridge = cxx_build::bridge("src/lib.rs");
    bridge
        .file(manifest.join("cpp/shim.cc"))
        .include(manifest.join("cpp"))
        .include(source.join("include"))
        .include(source.join("external/utfcpp/source"))
        .define("NT_DICT_BRIDGE_HEADER", "\"nihongo-dict/src/lib.rs.h\"")
        .std("c++20");
    if msvc {
        bridge.flag("/utf-8").flag("/Zc:__cplusplus");
    }
    bridge.compile("nihongo-dict-shim");
}

/// An explicit override wins, then whatever is on PATH.
fn tool(env_var: &str, name: &str) -> PathBuf {
    std::env::var(env_var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(name))
}

/// CMake needs the MSVC environment (INCLUDE, LIB, PATH) that `cc` already knows
/// how to find, so it is copied over instead of relying on a vcvars shell.
fn command(program: &Path, target: &str, msvc: bool, defines: bool) -> Command {
    let mut command = Command::new(program);
    if msvc {
        if let Some(tool) = cc::windows_registry::find_tool(target, "cl.exe") {
            // `cmake --build` rejects -D arguments, so the compiler is pinned
            // only while configuring.
            if defines {
                command.arg(format!("-DCMAKE_C_COMPILER={}", tool.path().display()));
                command.arg(format!("-DCMAKE_CXX_COMPILER={}", tool.path().display()));
            }
            for (key, value) in tool.env() {
                command.env(key, value);
            }
        }
    }
    command
}

fn run(mut command: Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("{what} could not start: {e}"));
    if !status.success() {
        panic!("{what} failed with {status}");
    }
}
