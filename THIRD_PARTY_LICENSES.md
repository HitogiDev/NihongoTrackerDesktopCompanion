# Third-party licenses

## Hoshidicts

The offline dictionary engine is included as the `crates/nihongo-dict/vendor`
submodule from the `main` branch of [Manhhao/hoshidicts](https://github.com/Manhhao/hoshidicts).
Hoshidicts main is licensed under GNU GPLv3. Its complete license text is in
`crates/nihongo-dict/vendor/LICENSE`, and its upstream acknowledgements identify
the licenses of the bundled dependencies.

The companion links Hoshidicts directly into its native binary. Anyone
redistributing a build should follow GPLv3 source, notice, and license
requirements for the combined work. The `main-mit` branch is a different
license choice and is not the engine used here.

## LunaHook / LunaHost

Windows native game text capture uses the unmodified x86 and x64 `LunaHook`
and `LunaHost` DLLs from LunaTranslator v10.16.1.2, as vendored by
[Fushi](https://github.com/hajisensai/Fushi/tree/main/native/galgame_hook/third_party/lunahook).
The matching source is at the
[LunaTranslator v10.16.1.2 tag](https://github.com/HIllya51/LunaTranslator/tree/v10.16.1.2).
The DLLs are dynamically loaded by an architecture-matched helper process.
LunaHook and LunaTranslator are licensed under GNU GPLv3; see
`src-tauri/native/game-hook/third_party/lunahook/LICENSE` and the pinned file
hashes in `src-tauri/native/game-hook/third_party/lunahook/VERSION.json`.

NTDC uses its own helper and IPC format. Its callback order, attachment sequence,
and split-context selection behavior were checked against Fushi's
`native/galgame_hook/include/luna_bridge.h`, `luna_text_selector.h`, and
`native/galgame_hook/injector/injector_main.cpp`. Fushi's complete native engine
adapters, audio capture, and overlay are not included.
