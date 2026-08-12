# CLAUDE.md

Guía para trabajar en este repositorio. El proyecto es el companion de escritorio
de **NihongoTracker** (repo hermano en `E:\Coding\NihongoTracker`).

## Comandos

```bash
npm run tauri dev      # app completa en desarrollo (Vite en :1420 + binario Rust)
npm run dev            # solo el frontend, sin acceso a comandos Tauri
npm run build          # tsc + vite build
npm run typecheck      # tsc --noEmit
npm run tauri build    # instalador (NSIS en Windows)
npm run test:rust      # cargo test (parser de releases y reproductores)
cd src-tauri && cargo check   # el ciclo rápido cuando tocas Rust
```

## Arquitectura

Toda la lógica de detección, red y persistencia vive en **Rust**. El frontend es
una vista: no habla con la API de NihongoTracker directamente.

```
src-tauri/src/
  lib.rs         Plugins, estado global, bandeja, registro de comandos
  state.rs       AppState (RwLock de tokio) + tipos de sesión compartidos
  config.rs      Settings serializables, con defaults
  db.rs          SQLite (rusqlite bundled): kv, vn_apps, outbox, pending_logs,
                 text_lines, text_state, watch_history
  platform/      Ventanas/procesos por SO. windows_impl.rs real, stub.rs el resto
  players.rs     Reproductores conocidos + limpieza del título de ventana
  parser.rs      Parser de nombres de release (tests dentro)
  watcher.rs     Bucle de detección; convierte sesiones en logs
  texthooker.rs  Cliente WebSocket Textractor/Agent
  api.rs         Cliente REST (x-api-key)
  sync.rs        Outbox con reintentos + push de líneas
  commands.rs    Superficie IPC
src/
  lib/ipc.ts     Único punto de entrada a los comandos Rust, tipado
  lib/types.ts   Espejo de los tipos de Rust — hay que mantenerlos sincronizados
  store/         zustand: settings, tracker, hooker (se alimentan de eventos)
  screens/       Dashboard, Queue, Library, Texthooker, Settings, Onboarding
  components/ui/ Design system copiado de NihongoTracker/Frontend
```

### Flujo de datos

1. `watcher.rs` sondea cada `poll_interval_ms` y publica un `TrackerSnapshot`
   por el evento `tracker://state`.
2. El frontend nunca hace polling: escucha eventos y llama comandos.
3. Los logs terminados van al **outbox** de SQLite, nunca directos a la red.
   `sync.rs` los drena cada 30 s y al confirmar desde la UI.

### Eventos Rust → frontend

`tracker://state`, `vn://started`, `vn://ended`, `texthooker://line`,
`texthooker://status`, `outbox://changed`, `pending://changed`,
`settings://changed`. Están centralizados en `src/lib/ipc.ts` (`events`).

## Reglas del proyecto

### Rust

- **Nada de bloquear el tick del watcher.** Las llamadas de red van en
  `tauri::async_runtime::spawn` (ver `request_media` y `flush_later`). Un
  `.await` de red dentro de `tick()` congela la detección.
- No mantengas un `RwLockReadGuard` de tokio vivo a través de un `.await` que
  pueda necesitar el write lock.
- `State<'_, AppState>` no vive más que la función: liga los guards a un `let`
  antes de devolver valores derivados.
- Todo lo que salga a la red pasa por `sync.rs`; los comandos no llaman a
  `ApiClient::create_log` directamente.
- El código específico de Windows va detrás de `#[cfg(windows)]` en
  `platform/`, con su equivalente en `stub.rs`. El resto del código no puede
  asumir Windows.

### Frontend

- daisyUI v5 + Tailwind v4. **Nunca interpoles clases** (`btn-${x}`): Tailwind
  escanea el texto fuente. Usa los mapas de `components/ui/Button.tsx`.
- Clases eliminadas en daisyUI v5 que no deben aparecer: `input-bordered`,
  `select-bordered`, `textarea-bordered`, `form-control`, `label-text`,
  `tabs-boxed`, `card-compact`. `loading` no es un modificador de `btn`.
- Solo dos elevaciones: `shadow-sm` y `shadow-lg`. Superficies con `surface`,
  `surface-muted` o `surface-raised`, nunca `bg-base-100 rounded-lg border` a mano.
- Colores por tipo de media: siempre desde `src/constants/mediaColors.ts`. Fuera
  de ahí, tokens semánticos de daisyUI, no la paleta cruda de Tailwind.
- Iconos de `lucide-react`, dimensionados por className (`w-4 h-4`), nunca con
  la prop `size`.
- Añadir un comando Rust implica: `commands.rs` → `invoke_handler` en `lib.rs`
  → wrapper en `src/lib/ipc.ts`. Los tres pasos, siempre.

## Contrato con el backend de NihongoTracker

Detalles verificados contra `E:\Coding\NihongoTracker\Backend` que es fácil
equivocarse:

- **Auth**: cabecera `x-api-key`, comprobada antes que la cookie JWT
  (`middlewares/authMiddleware.ts`). La cookie es `httpOnly` + `sameSite: strict`
  y no sirve para un cliente nativo.
- **CORS**: allowlist fija (`localhost:5173` y el dominio de producción). Por eso
  las peticiones salen desde Rust con `reqwest` y no desde el webview.
- `POST /api/logs` exige **`description`** siempre, aunque el esquema la tenga
  como opcional. Devuelve **200**, no 201.
- El `xp` que envíe el cliente se ignora: lo recalcula el middleware
  `calculateXp`. Para estimar, `POST /api/logs/preview-xp`.
- Los errores de validación llegan como `{ message: "Validation Error" }` sin
  decir qué campo falla. Valida en el cliente.
- Campos obligatorios por tipo: `anime` → `episodes`; `vn` → `chars` o `time`;
  `manga`/`reading`/`book` → `pages`, `chars` o `time`; `video`/`movie`/`audio`
  → `time`. `time` siempre en **minutos**.
- `mediaId` es el `contentId` de la media, no un ObjectId: AniList numérico para
  anime/manga, `v<n>` para novelas visuales, `igdb-<id>` para juegos,
  `gbooks-<id>` para libros.
- `/api/media/search` usa la ortografía `tv_show` (con guion bajo) mientras que
  el tipo de log es `tv show` (con espacio). Conversión en `api::search_type_for`.
- No hay endpoint de lote ni clave de idempotencia: cada log es una petición y la
  deduplicación es responsabilidad nuestra (`watch_history` en SQLite).
- Texthooker: `POST /api/texthooker/:contentId/lines` espera `charsCount`,
  mientras que el formato de socket de la web usa `japaneseCount`.

## Al tocar la detección

- El parser tiene tests en `parser.rs`. Si añades un formato de nombre, añade el
  caso ahí antes de tocar la lógica.
- Los reproductores nuevos van a la tabla `PLAYERS` de `players.rs`, con sus
  sufijos de título y, si aplica, su título en reposo en `IDLE_TITLES`.
- El conteo de caracteres debe seguir siendo solo kana + kanji
  (`texthooker::count_japanese`), igual que `countJapaneseCharacters` en la web,
  o las estadísticas dejarán de cuadrar con el sitio.
