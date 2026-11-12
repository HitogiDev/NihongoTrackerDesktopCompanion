# Upstream PR: split the deconjugation rules literal so MSVC can compile it

Not opened from here: this environment has no `gh` and no GitHub credentials.
Everything the PR needs is below; it is one commit against `main-mit`.

## Where

`Manhhao/hoshidicts`, branches `main` and `main-mit` (the file is identical in
both).

## Title

    fix: split the deconjugation rules literal so MSVC can compile it

## Body

    `src/deconjugator/deconjugation_rules.hpp` holds the rule set as a single
    raw string literal of about 35 KB. MSVC rejects it:

        deconjugation_rules.hpp(923): error C2026: string too big, trailing
        characters truncated
        deconjugation_rules.hpp(1649): error C2026: string too big, trailing
        characters truncated

    MSVC caps a single string literal at 16,380 bytes. The limit on the
    *concatenated* result of adjacent literals is 65,535 bytes, so splitting the
    literal into five adjacent chunks compiles cleanly and leaves the resulting
    `std::string_view` byte-for-byte identical.

    No rule is touched; the cuts are at line boundaries inside the JSON. Verified
    with MSVC 14.50 (VS Build Tools 2026) and GCC 15.3 — GCC builds both the
    original and the split version, so this costs nothing on other compilers.

    Two more flags are needed on MSVC and are not part of this change, in case
    they are useful to note in the README: `/utf-8` (otherwise the source is
    decoded with the system code page and the Japanese is mangled) and
    `/Zc:__cplusplus` (otherwise `__cplusplus` reports 199711L and utfcpp hides
    the C++17 API `text_processor.cpp` calls).

## The commit

`0001-msvc-split-string-literal.patch` in this directory, generated from a clean
checkout of `af99b55`. Apply with:

    git apply crates/nihongo-dict/patches/0001-msvc-split-string-literal.patch

Nine lines change. `split-rules-literal.mjs` regenerates it if the rules file
moves on upstream.

## After it merges

`build.rs` notices a tree that is already split and does nothing, so the
auto-patch can stay in place until the submodule is bumped past the merge, and
then be deleted.
