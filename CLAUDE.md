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
npm test               # vitest: lógica pura y stores del frontend
npm run check:i18n     # en y es con las mismas claves y {{placeholders}}
npm run bindings       # regenera src/lib/bindings desde los tipos de Rust
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
  platform/      Ventanas, foco, inactividad y reproductores por SO: windows_impl,
                 linux_impl (X11) + dbus (MPRIS, inactividad en Wayland), macos_impl
  players.rs     Reproductores conocidos + limpieza del título de ventana
  parser.rs      Parser de nombres de release (tests dentro)
  playback.rs    Posición real: IPC de mpv y interfaz web de MPC
  mpv_config.rs  Activa input-ipc-server en el mpv.conf del usuario (con su permiso)
  watcher.rs     Bucle de detección; convierte sesiones en logs
  texthooker.rs  Cliente WebSocket Textractor/Agent + lector de portapapeles
  api.rs         Cliente REST (x-api-key)
  streak.rs      Aviso de "racha en riesgo": day-keys, ventana y antirruido
  discord.rs     Rich Presence opcional: política de privacidad + hilo de IPC
  i18n.rs        Los pocos textos que muestra Rust (bandeja y notificaciones)
  sync.rs        Outbox con reintentos + push de líneas
  commands.rs    Superficie IPC
packages/
  yomitan-content/  Renderer de structured content de Yomitan (TS), compartido
                    con el Frontend web. Sin Tauri dentro
src/
  i18n/          i18next: `locales/{en,es}/*.json`, un namespace por pantalla
  lib/ipc.ts     Único punto de entrada a los comandos Rust, tipado
  lib/bindings/  Tipos generados por ts-rs desde los structs de Rust (no editar)
  lib/types.ts   Reexporta bindings/ + lo que Rust pasa sin tipar (respuestas de la API)
  lib/logRules.ts Campos obligatorios por tipo de log, compartidos por todos los formularios
  store/         zustand: settings, tracker, hooker (se alimentan de eventos)
  screens/       Dashboard, Queue, Library, Texthooker, Settings, Onboarding
  components/ui/ Design system copiado de NihongoTracker/Frontend
```

### Flujo de datos

1. `watcher.rs` sondea cada `poll_interval_ms` y publica un `TrackerSnapshot`
   por el evento `tracker://state`.
2. El frontend nunca hace polling: escucha eventos y llama comandos.
3. Los logs terminados van al **outbox** de SQLite, nunca directos a la red.
   `sync.rs` los drena cada 30 s y al confirmar desde la UI. Cada drenaje va
   bajo `AppState::outbox_flush`: sin él, dos drenajes simultáneos leen las
   mismas filas y el log sale dos veces. Además cada envío lleva el id de la
   fila como `Idempotency-Key`, así que reintentar tras una respuesta perdida
   tampoco duplica. Editar una fila (`update_outbox`) toma el mismo lock: un
   drenaje en curso tiene el payload viejo en memoria.
4. El cronómetro de una sesión suma el tiempo real entre ticks (`Instant`), no
   el intervalo configurado: el resto por debajo del segundo se arrastra en
   `elapsed_carry_ms`.

### Quién manda en cada cosa

- La sesión del texthooker (`content_id`) la fija **Rust** al detectar una VN, no
  el frontend: la ventana puede estar en la bandeja y su listener no dispararse.
- El cronómetro de una VN detectada lo lleva el watcher; el temporizador manual
  de la pantalla del texthooker solo se usa cuando no hay sesión automática.
- El XP nunca se calcula en el cliente: se pide a `POST /api/logs/preview-xp`.
- El día del usuario lo decide `streak.rs`, no el frontend ni el reloj del SO.

### Ventanas

Cada webview es un proceso de renderizado, así que solo existen cuando hacen falta:

- `main` se construye oculta (`build_main_window`) y **se muestra desde el
  frontend** tras el primer render, para que no haya fogonazo en blanco. Rust
  la muestra igualmente a los 4 s si la página no carga. Con `start_minimized`
  no se construye hasta que la bandeja o otra instancia la piden
  (`show_main_window`).
- Por eso un evento dirigido a `main` puede no tener a nadie escuchando. Lo que
  la abre con un encargo (log de racha, navegación desde el overlay) pasa por
  `open_main_with`: evento si ya existe, `main_intent` si hubo que crearla, y la
  página lo recoge con `take_main_intent`. Lo que deba pasar aunque `main` no
  exista (abrir el texthooker al detectar una VN) lo hace Rust.
- El overlay de sesión existe solo mientras `overlay_enabled`; apagarlo lo
  destruye. El overlay de lectura y su popup se crean la primera vez que se
  quieren para una VN y viven hasta el cierre: las ventanas de ajustes son
  hijas del popup y morirían con él.
- El estado del overlay de lectura (`reading_overlay::Runtime`, en `AppState`)
  existe aunque sus ventanas no: la última línea y el hook nativo de la obra
  (`OverlayState::hook`, por `content_id`, venga de donde venga la selección)
  los fija Rust (`sync_hook` en cada tick y al elegir hook). El overlay no lee
  `tracker.game_hook`, así que si se crea con la VN ya hookeada desde el
  texthooker, arranca enganchado.
- El overlay de lectura es `focusable(false)`, así que sus teclas van al juego
  y una VN salta texto con Ctrl. El **escudo de Ctrl**
  (`platform::shield_ctrl`, `reading_overlay::ctrl_shield_blocks`) se traga
  las pulsaciones de Ctrl mientras el cursor está sobre el overlay o su popup,
  el juego tiene el foco y no está en click-through
  (`reading_overlay.block_ctrl_on_hover`). En Windows es un `WH_KEYBOARD_LL`
  en su propio hilo, instalado solo mientras el overlay está visible. Cada
  pulsación y su suelta van por el mismo camino (`platform::ctrl_event`): si
  no, el juego vería un Ctrl pulsado que nunca se suelta y saltaría sin fin. Si
  se pulsa otra tecla con un Ctrl tragado, es un atajo (Ctrl+Shift+I): se
  reinyectan el Ctrl y la tecla con `SendInput` y ese Ctrl deja de tragarse.
  En Linux y macOS es un no-op (`Capabilities::ctrl_shield`).
- `window_chrome.rs`: barra de título con el tema de daisyUI y Mica tras la
  sidebar en Windows 11 (`data-vibrancy` en `<html>`; sin él no cambia nada).
- Cada pantalla es un chunk (`React.lazy` en `App.tsx`): todas las ventanas
  cargan el mismo bundle.
- En producción `src/lib/desktop.ts` anula los atajos y el menú contextual del
  navegador. Los atajos de la app (Ctrl/⌘+1…6, `,`, `L`, `T`) viven en `Sidebar`.

### Actualizaciones

`updater.rs` sobre `tauri-plugin-updater`, contra
`releases/latest/download/latest.json` del repo de GitHub. Lo publica
`.github/workflows/release.yml` al empujar un tag `v*` (la versión sale de
`tauri.conf.json`).

- Busca a los 30 s del arranque y cada 6 h si `auto_update_check`; avisa una vez
  por versión (`kv:updater:notified_version`). **Instalar siempre lo decide el
  usuario**, desde Ajustes → App o el aviso de la sidebar.
- La clave pública está en `plugins.updater.pubkey`; la privada no está en el
  repo: va en el secreto `TAURI_SIGNING_PRIVATE_KEY`. Si se pierde, las copias
  instaladas no aceptarán ninguna release nueva.
- En macOS el actualizador necesita el `.app.tar.gz`, que solo sale con `"app"`
  en `bundle.targets`: sin él `latest.json` no tiene entradas `darwin-*` y
  los Mac nunca se actualizan. El `.dmg` es solo para la primera instalación.
- Las notas salen de los títulos de los PRs (`generate-notes`, sin los
  `chore: release`). En `latest.json`, que la app enseña como texto plano, van
  solo los títulos; la página de la release añade la tabla de descargas
  (job `downloads`). Así que el título del PR es lo que lee el usuario.
- Los archivos se llaman `NihongoTrackerDesktopCompanion_<versión>_…`
  (`assetNamePattern`): con el `productName` GitHub cambia el espacio por un punto.
- Solo la AppImage se reemplaza sola en Linux; con .deb/.rpm la UI enlaza a la
  release (`UpdateInfo::can_install`).
- `tauri-plugin-updater` está fijado a `~2.12`: desde 2.13 exige Tauri 2.12.
- El repo tiene que ser público: la app descarga las releases sin autenticarse.

### Recordatorio de racha

`streak.rs` avisa antes de que el día acabe **en la zona horaria de la cuenta**
(`user.settings.timezone`, por defecto `UTC`), no en la del sistema operativo. La
day-key `YYYY-MM-DD` tiene que salir idéntica a la del backend
(`Backend/src/services/streaks.ts`), y hay un test con valores calcados de su
`Intl` para que no se desvíe.

- `lastStreakDate` es un **ancla de medianoche UTC** de una day-key, así que se
  lee en UTC (`getDayKeyFromUTCDate`). Convertirla a la zona del usuario correría
  el día para todo el que esté al oeste de Greenwich.
- Está en riesgo si `currentStreak > 0` y la day-key del ancla no es la de hoy.
- Los logs con `unknownDate: true` no cuentan para la racha: "he enviado un log
  hoy" no prueba nada, sólo lo hace `lastStreakDate`.
- Antirruido: una notificación por day-key como mucho (persistida en
  `kv:streak:notified_day`), y **siempre se reconsulta** justo antes de
  dispararla, porque el log pudo entrar desde la web o el móvil.
- Presupuesto de red: `streak::budget` decide la antigüedad admisible —5 min
  dentro de la ventana, 1 h fuera— y **todo** el mundo pide por `streak::refresh`,
  incluida la UI, así que abrir el dashboard no gasta peticiones extra.
- Un 401 apaga el bucle (`streak_auth_failed`) y emite `streak://auth-required`;
  sólo `save_settings` lo rearma. Nada de reintentar una clave revocada.
- La notificación es accionable en Windows y Linux: el plugin de Tauri no expone
  el clic en escritorio. En Windows el toast se construye con
  `tauri-winrt-notification` (botón + `on_activated`), con el AppUserModelID de
  la app y `POWERSHELL_APP_ID` de reserva; en Linux con `notify-rust` y
  `wait_for_action` en un hilo. En macOS cae al plugin, sin botón. El clic (y la
  entrada de la bandeja) emiten `streak://open-log`.

### Extensiones del webview

La ventana principal **no** está en `tauri.conf.json`: se construye en
`build_main_window` porque `extensions_path` solo existe en el builder de Rust,
no en el archivo de configuración. De ahí salen los diccionarios emergentes
(Yomitan, JitenReader) cargados como extensiones sin empaquetar. Solo Windows.

El id de una extensión sin empaquetar lo deriva Chromium de su ruta absoluta
(SHA-256, primeros 16 bytes, cada dígito hex mapeado `0-f` → `a-p`; en Windows
la ruta se hashea como UTF-16LE). Está en `commands::extension_id_for_path` y
sirve para abrir sus páginas internas, ya que el webview no tiene barra de
herramientas.

### Salas del texthooker

Las salas compartidas van por Socket.IO **desde el webview** (`src/lib/room.ts`),
no desde Rust, porque el protocolo es el mismo que el de la web y así una sala
sirve para los dos clientes. El handshake del socket autentica con la cookie
`jwt`, que aquí no existe, pero el servidor la trata como opcional y
`join_room` acepta el `username` en el propio evento. El `hostToken` se guarda
en `localStorage` con la misma clave que la web (`hostToken_<roomId>`), así que
retomar una sala funciona indistintamente desde el escritorio o el navegador.
Ojo con el formato: por socket las líneas viajan con `japaneseCount`.

### Discord Rich Presence

Opt-in y apagado por defecto: con el toggle apagado no se lanza el hilo ni se
abre ningún socket. Lo que se publica lo decide `discord::decide`, una **función
pura**, y la pantalla de ajustes llama exactamente a esa misma función para el
preview: lo que ve el usuario ahí es literalmente lo que verán los demás, no una
maqueta.

Orden de la política, y no se puede reordenar:

1. Obra o proceso en la lista negra → no se publica **nada**, ni genérico.
2. `isAdult`, media sin resolver, o el modo genérico global → presencia genérica
   ("Inmersión en japonés"), sin título ni portada. El progreso (episodio o
   caracteres) sí se queda: no identifica nada.
3. Si no, la presencia completa.

- `isAdult` viaja con los datos, no se consulta al vuelo: `MediaMatch.is_adult`
  y la columna `vn_apps.is_adult` (migración en `db::MIGRATIONS`, porque SQLite
  no tiene `ADD COLUMN IF NOT EXISTS`). Si añades otra ruta que cree un
  `MediaMatch` o un `VnApp`, arrastra el flag o el título acabará en Discord.
- Una VN en pausa (`counting == false`) limpia la presencia **salvo** que
  `discord_show_idle` esté activo (por defecto sí): entonces la mantiene con el
  texto "idling" ("Inactivo en <título>") y la portada, pero la lista negra le
  sigue aplicando igual. El reproductor de vídeo en pausa siempre limpia.
  También se limpia al cerrar la app (`RunEvent::Exit`) y al apagar el toggle.
- El IPC del crate es **bloqueante**, así que vive en su propio hilo de SO con un
  `std::sync::mpsc`. Nunca en un worker de tokio, y un fallo de IPC no puede
  llegar al watcher.
- Límite de Discord: ~1 update cada 15 s. El hilo guarda un único `pending` que
  se **sobrescribe**, así que los estados intermedios se descartan, no se
  encolan. Los `clear` se saltan el límite: una presencia colgada es una fuga.
- Reconexión con backoff 2 s → 60 s. Que Discord no esté abierto no es un error,
  y no bloquea el arranque.
- `timestamps.start` es el inicio de la sesión: cuenta Discord, no nosotros.
- El Application ID lo pone el usuario en ajustes (hay que crear la app en el
  Developer Portal y subir los assets); sin él no se intenta conectar.

### Renderer de diccionario

`packages/yomitan-content` es un paquete TypeScript **sin nada de Tauri**: lo
consume este webview (`file:packages/yomitan-content`) y también el Frontend web
(por ruta relativa entre repos hermanos). Rust devuelve el JSON del glosario tal
cual; todo lo que es presentación vive ahí, para que los dos clientes enseñen una
definición igual.

Lo que hay que saber antes de tocarlo:

- El alcance está cerrado por el censo de las 435.448 entradas de Jitendex: 8
  tags, los once hooks por encima del 10 %, tres `data-sc-class`. `table` e `img`
  quedan fuera **de momento** — no se descartan, se renderiza su contenido y se
  cuenta en `RenderDiagnostics`.
- **Nunca `innerHTML` ni `dangerouslySetInnerHTML`**, y **nunca se emite un
  `href`**: un `<a>` interno (`?query=…`) se convierte en `<button>` que pide otro
  lookup, y cualquier otro se queda en texto. Por eso no hay lista negra de
  esquemas: `javascript:` simplemente no es la forma interna.
- El `styles.css` del diccionario se pasa por `scopeDictionaryCss` y va en un
  `<style>` dentro del popup. Nunca global: sale de un zip que importó el usuario.
- Los mapas de clases se reparten así: `data-sc-class` pone la forma (badge,
  caja) y `data-sc-content` solo color y tipografía, porque en los datos
  `part-of-speech-info` siempre viene con `tag` y `xref` siempre con `extra-box`.
  Si los dos ponen badge, sale `badge-sm badge-xs` con dos colores peleándose.
- El paquete no tiene ni una cadena visible: el botón de volver y sus textos los
  pone cada app con su i18n. `useLookupHistory` solo lleva la pila.

### Copia de seguridad

`export_database` hace `VACUUM INTO` (copia consistente aunque haya WAL) y
`restore_database` copia el archivo encima de la conexión viva con la API de
backup de SQLite, la migra y **reinicia la app**, para que ningún bucle siga con
los datos viejos. Antes guarda la actual como `companion.before-restore.sqlite3`.
Rechaza cualquier archivo sin las tablas `kv`, `vn_apps` y `outbox`. La copia
lleva la clave de API (los ajustes viven en `kv`); los diccionarios y las
capturas no van dentro.

### Eventos Rust → frontend

`tracker://state`, `vn://started`, `vn://ended`, `texthooker://line`,
`texthooker://status`, `texthooker://timer`, `outbox://changed`,
`pending://changed`, `settings://changed`, `log://celebration`,
`streak://reminder`, `streak://open-log`, `streak://auth-required`,
`discord://status`, `updater://available`, `updater://progress`. Están
centralizados en `src/lib/ipc.ts` (`events`).

`log://celebration` lleva el `celebration` y los `newAchievements` que devuelve
`POST /api/logs`; lo emite `sync.rs` al entregar del outbox, así que un log que
se encoló sin conexión celebra igual cuando por fin sale.

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
- El código específico de un SO va en `platform/`: una función nueva se
  implementa en `windows_impl`, `linux_impl`, `macos_impl` **y** `stub`. El resto
  del código no puede asumir Windows.
- Lo que cada SO puede observar lo dice `platform::capabilities()`, y la UI lo
  enseña en Ajustes → Seguimiento. Si algo no se puede saber (el foco en
  Wayland), se asume el caso favorable: un juego que exige foco no contaría nunca.
- Los nombres de proceso salen de `platform::process_key`, no de
  `Process::name()`: en Linux `name()` es el `comm` truncado a 15 caracteres y un
  juego de Wine se llama `wine64-preloader`. La biblioteca, el watcher y el
  minado tienen que usar la misma clave.
- Desde Windows no se puede compilar la app para Linux o macOS (GTK y los SDK),
  pero `platform/` sólo depende de crates en Rust puro: se comprueba con un crate
  que lo incluya por `#[path]` y `cargo check --target x86_64-unknown-linux-gnu`
  / `aarch64-apple-darwin`. La app entera la compila el CI en los tres sistemas.

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
- Un struct que Rust manda a la UI lleva
  `#[cfg_attr(test, derive(ts_rs::TS), ts(export))]` y su tipo TS sale de
  `src/lib/bindings/` (reexportado en `types.ts`), nunca copiado a mano. Tras
  cambiarlo, `npm run bindings` y commitear el resultado: el CI falla si
  `bindings/` no coincide. Solo queda escrito a mano lo que Rust no tipa
  (respuestas de la API que pasan como `Value`, objetos hechos con `json!`).
- Los campos obligatorios de un log por tipo están en `src/lib/logRules.ts` y
  los usan el registro rápido y el editor de la cola (`LogForm`).
- **Ningún texto visible se escribe a pelo.** Todo pasa por `useTranslation` y
  vive en `src/i18n/locales/en` y `.../es`; las dos carpetas tienen que tener
  exactamente las mismas claves (lo comprueba `npm run check:i18n` en el CI).
  Inglés es el idioma de respaldo.
- Los textos que enseña Rust (bandeja, notificaciones) están en
  `src-tauri/src/i18n.rs` y hay que mantenerlos a mano en los dos idiomas. El
  menú de la bandeja se reconstruye en `save_settings` al cambiar de idioma.

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
- Los errores de validación de mongoose llegan como `{ message: "Validation
  Error: <rutas>", fields: { ruta: motivo } }` (los servidores anteriores, solo
  `"Validation Error"`). `api.rs` añade los motivos al mensaje que ve la cola.
  Igualmente valida en el cliente (`logRules.ts`).
- Campos obligatorios por tipo: `anime` → `episodes`; `vn` → `chars` o `time`;
  `manga`/`reading`/`book` → `pages`, `chars` o `time`; `video`/`movie`/`audio`
  → `time`. `time` siempre en **minutos**.
- `mediaId` es el `contentId` de la media, no un ObjectId: AniList numérico para
  anime/manga, `v<n>` para novelas visuales, `igdb-<id>` para juegos,
  `gbooks-<id>` para libros.
- `/api/media/search` usa la ortografía `tv_show` (con guion bajo) mientras que
  el tipo de log es `tv show` (con espacio). Conversión en `api::search_type_for`.
- No hay endpoint de lote: cada log es una petición. `POST /api/logs` acepta
  `Idempotency-Key` (`[a-zA-Z0-9._:-]{1,128}`) y ante una clave repetida
  devuelve el log que ya creó; mandamos el id del outbox. Que un episodio no se
  registre dos veces sigue siendo cosa nuestra (`watch_history` en SQLite).
- Texthooker: `POST /api/texthooker/:contentId/lines` espera `charsCount`,
  mientras que el formato de socket de la web usa `japaneseCount`.

## Al tocar la detección

- El parser tiene tests en `parser.rs`. Si añades un formato de nombre, añade el
  caso ahí antes de tocar la lógica.
- Los reproductores nuevos van a la tabla `PLAYERS` de `players.rs`, con sus
  sufijos de título y, si aplica, su título en reposo en `IDLE_TITLES`.
- La posición real manda sobre el tiempo transcurrido: `AnimeSession::ready`
  usa `position/duration` cuando el reproductor la da y solo cae al reloj si no.
  Un reproductor nuevo con API se añade a `playback::probe`, no al watcher.
- El sondeo de posición sale del tick (`request_playback`) con un `AtomicBool`
  que impide que se acumulen sondas si el reproductor tarda.
- `mpv_config.rs` configura el IPC de mpv por el usuario, **siempre tras
  confirmar** (aviso en la tarjeta de anime del Dashboard si mpv lleva 5 s sin
  dar posición, `needsMpvSetup`, y botón en Ajustes). Elige el `mpv.conf` que
  mpv lee: `portable_config` junto al ejecutable en marcha, si no
  `%APPDATA%\mpv` (`mpv.net` para mpv.net), `$XDG_CONFIG_HOME/mpv` o
  `~/.var/app/io.mpv.Mpv/config/mpv` si el proceso es un Flatpak (exe bajo
  `/app`), cuyo socket va en `$XDG_RUNTIME_DIR/app/io.mpv.Mpv/` porque su `/tmp`
  es privado. Un `input-ipc-server` de nivel superior que ya exista **manda**:
  se adopta su ruta en vez de reescribirlo. Si no, añade la línea con un
  comentario antes del primer perfil y guarda el original una sola vez como
  `mpv.conf.before-nihongotracker`. mpv solo lo lee al arrancar: hay que
  reiniciarlo. IINA no lee `mpv.conf` por defecto y no se sondea.
- El conteo de caracteres debe seguir siendo solo kana + kanji
  (`texthooker::count_japanese`), igual que `countJapaneseCharacters` en la web,
  o las estadísticas dejarán de cuadrar con el sitio.
- Las dos entradas del texthooker (WebSocket y portapapeles) terminan en
  `texthooker::ingest`. Cualquier filtro nuevo va ahí, no duplicado en cada
  camino. El del portapapeles además exige que la copia lleve japonés, o cada
  Ctrl+C acabaría siendo una línea.
- El filtro de chino (`texthooker_filter_chinese`, `text_filters::drop_chinese`)
  es global, pero cada obra de la biblioteca puede activarlo o desactivarlo
  para sí (`vn_apps.filter_chinese`, `NULL` = el global; lo consulta `ingest`
  por `content_id`). Juzga línea a línea: con kana nunca es chino; sin kana, solo si tiene 4+ kanji
  y una marca china (`CHINESE_MARKERS`) o 12+ kanji. Las marcas excluyen a
  propósito kanji de cabeceras japonesas (都, 過, 着…): añade casos al test
  antes de tocar la lista. Una línea con kana puede traer la traducción pegada
  (「え…」“啊……”): `drop_glued_chinese` se queda desde la frase del primer kana
  hasta el cierre de la del último, y quita lo de los lados si es chino. Un
  “…” pegado a un 「…」 basta; si no, hace falta una marca o 10+ kanji.
- `src/screens/Texthooker.tsx` es un port de `HookerScreen.tsx` de la web: mismo
  layout, mismas clases (`th-root`, `th-topbar`, `th-line-item`…), mismas
  opciones de fuente y tema, mismo cronómetro anclado a reloj de pared. Si
  cambias algo ahí, compáralo con el original antes.
- El estado del cronómetro vive en los dos lados: la UI lo lleva y lo replica en
  `AppState::hooker_timer_active` con `hooker_set_timer_active`, porque la
  ingestión en Rust necesita saberlo para los ajustes de "en pausa" y de
  arranque automático.
