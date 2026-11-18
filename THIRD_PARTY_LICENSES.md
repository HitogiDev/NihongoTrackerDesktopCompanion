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

## Built-in manga OCR (downloaded on demand)

None of these files ship with the app. They are downloaded into the app's
data folder (`ocr/`) only after the user agrees to it in the OCR dialog, and
each is checked against the SHA-256 pinned in
`src-tauri/src/manga/ocr/assets.rs`. "Delete models" removes them.

- **ONNX Runtime 1.24.4** — Copyright (c) Microsoft Corporation, MIT License.
  The unmodified native library from Microsoft's official NuGet packages
  [`Microsoft.ML.OnnxRuntime`](https://www.nuget.org/packages/Microsoft.ML.OnnxRuntime/1.24.4)
  (Linux, macOS) and
  [`Microsoft.ML.OnnxRuntime.DirectML`](https://www.nuget.org/packages/Microsoft.ML.OnnxRuntime.DirectML/1.24.4)
  (Windows), built from [microsoft/onnxruntime](https://github.com/microsoft/onnxruntime)
  `v1.24.4`. On Intel Macs, which 1.24 no longer builds for, **ONNX Runtime
  1.23.2** instead, from
  [`Microsoft.ML.OnnxRuntime` 1.23.2](https://www.nuget.org/packages/Microsoft.ML.OnnxRuntime/1.23.2)
  (`v1.23.2`, same license). On Windows also `DirectML.dll` from
  [`Microsoft.AI.DirectML` 1.15.4](https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4),
  under the license included in that package. The app talks to ONNX Runtime
  through the [`ort`](https://github.com/pykeio/ort) crate (MIT OR
  Apache-2.0), which loads the library at run time instead of linking it.
- **comic-text-and-bubble-detector** —
  [ogkalu/comic-text-and-bubble-detector](https://huggingface.co/ogkalu/comic-text-and-bubble-detector),
  Apache License 2.0. The unmodified `detector_int8.onnx` (RT-DETR-v2).
- **manga-ocr** — [kha-white/manga-ocr-base](https://huggingface.co/kha-white/manga-ocr-base)
  (Apache License 2.0, Copyright Maciej Budyś), as converted to ONNX in
  [onnx-community/manga-ocr-base-ONNX](https://huggingface.co/onnx-community/manga-ocr-base-ONNX)
  (Apache License 2.0): the unmodified encoder and decoder files and the
  original `vocab.txt`. The preprocessing, beam-search settings and text
  post-processing reproduced in `src-tauri/src/manga/ocr/` follow
  [kha-white/manga-ocr](https://github.com/kha-white/manga-ocr) (Apache
  License 2.0); its half-width to full-width table follows
  [jaconv](https://github.com/ikegami-yukino/jaconv) (MIT License).

The GPL-3.0 comic-text-detector used by mokuro is not used.

## Manga extensions server (downloaded on demand)

Neither file ships with the app. In the managed mode they are downloaded into
the app's data folder (`suwayomi/`) only after the user agrees to it in the
install dialog, each checked against the SHA-256 pinned in
`src-tauri/src/manga/suwayomi/assets.rs`, and run as a separate process.
"Remove server" in settings deletes them.

- **Suwayomi-Server v2.4.2366** — Copyright the Suwayomi Project
  contributors, [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/).
  The unmodified release jar from
  [Suwayomi/Suwayomi-Server releases](https://github.com/Suwayomi/Suwayomi-Server/releases/tag/v2.4.2366);
  its source code is at
  [github.com/Suwayomi/Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server/tree/v2.4.2366).
  The app only talks to it over HTTP (its GraphQL API) on 127.0.0.1; no
  code of it is copied into the app.
- **Eclipse Temurin 21 JRE** (21.0.13+9; 21.0.12.1+1 on Windows on Arm) —
  the Adoptium Working Group, GNU General Public License version 2 with the
  Classpath Exception. The unmodified binaries from
  [adoptium/temurin21-binaries](https://github.com/adoptium/temurin21-binaries/releases),
  built from [OpenJDK](https://openjdk.org/); used only to run Suwayomi-Server.

Extensions installed in that server (for example from the
[Keiyoushi](https://keiyoushi.github.io) store) are third-party code under
their own licenses; the app neither ships nor modifies them.
