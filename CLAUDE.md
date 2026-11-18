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
  fushi.rs       Lee qué reproduce Fushi de su propia SQLite (solo lectura)
  watcher.rs     Bucle de detección; convierte sesiones en logs
  texthooker.rs  Cliente WebSocket Textractor/Agent + lector de portapapeles
  api.rs         Cliente REST (x-api-key)
  streak.rs      Aviso de "racha en riesgo": day-keys, ventana y antirruido
  discord.rs     Rich Presence opcional: política de privacidad + hilo de IPC
  i18n.rs        Los pocos textos que muestra Rust (bandeja y notificaciones)
  sync.rs        Outbox con reintentos + push de líneas
  commands.rs    Superficie IPC
  manga/         Lector de manga: biblioteca local, CBZ, .mokuro, protocolo manga://,
                 sesión de lectura, logs y minado con recorte (session.rs, crop.rs)
    webdav/      Fuentes WebDAV: client (HTTP y credenciales), propfind, layout,
                 sync (listado a la biblioteca), cache (LRU), ranged (CBZ por
                 rangos como Read+Seek), remote (de dónde sale cada archivo, descargas)
    ocr/         OCR integrado (ONNX vía `ort`): assets (archivos y SHA-256),
                 download, engine, beam, text, layout, worker (cola), export;
                 owocr (cliente WebSocket del owocr del usuario + conversión)
    suwayomi/    Extensiones vía Suwayomi-Server: assets (jar y JRE con SHA-256),
                 install, process (arranque headless), graphql (cliente),
                 catalog (lo que ve la UI, filtro adulto), library (series y
                 capítulos en la biblioteca)
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
  screens/       Dashboard, Queue, Library, Manga, Texthooker, Settings, Onboarding
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
  navegador. Los atajos de la app (Ctrl/⌘+1…7, `,`, `L`, `T`) viven en `Sidebar`.

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
- La AppImage que sube tauri-action lleva el `libwayland` de Ubuntu, que choca
  con una Mesa más nueva (Arch: `EGL_BAD_PARAMETER` y ventana en blanco). El
  job `appimage` lo quita (`scripts/appimage-unbundle-wayland.sh`), la vuelve a
  firmar y corrige la firma en `latest.json`, después de todos los builds para
  no pisarse con ellos. `WEBKIT_DISABLE_DMABUF_RENDERER` solo no basta.
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

### Lector de manga

Biblioteca local y lector en `src-tauri/src/manga/` y `src/screens/Manga.tsx` +
`src/screens/manga/`. El plan completo (WebDAV, OCR, Suwayomi) está en
`docs/plan-lector-manga.md`; están hechas todas las fases, de M0 a M7.

- **El webview no lee el disco.** Páginas, miniaturas y portadas van por el
  protocolo `manga://` (`protocol.rs`, registrado en `lib.rs`):
  `page/<volumen>/<n>`, `thumb/<volumen>/<n>`, `cover/<serie>`. Solo ids
  opacos; el archivo sale de la base de datos, nunca de la URL. `parse` solo
  decodifica `%2F` (lo que hace `convertFileSrc`) y rechaza cualquier otro
  carácter: no añadas formas que acepten rutas. Las páginas de una carpeta se
  vuelven a comprobar dentro de la carpeta tras resolver enlaces. La URL la
  construye `src/lib/manga.ts` (`mangaUrl`), que deja a `convertFileSrc` el
  esquema de cada SO (`http://manga.localhost` en Windows).
- **CBZ desconfiado** (`archive.rs`): nombres con `..`, absolutos o con `:`
  no son páginas; una entrada que infla más de 50:1 (por encima de 2 MB) es
  una bomba; toda lectura corta a 64 MB diga lo que diga la cabecera. Orden
  natural (`natural_cmp`, carpeta a carpeta) sin `__MACOSX` ni ocultos.
- **Detección** (`library.rs`): carpeta con imágenes = tomo, `.cbz`/`.zip` =
  tomo, `<Vol>.mokuro` junto a `<Vol>.cbz` o a `<Vol>/` = texto, la carpeta que
  lo contiene = serie. `<Vol>.webp` y `cover.*` son portadas, no páginas. Los
  ids son UUID v5 de la ruta (`stable_id`): el progreso (`manga_progress`, sin
  clave foránea a propósito) sobrevive a un reescaneo y a un disco que vuelve.
  Una raíz que no existe se salta, nunca se vacía. Raíces solapadas se
  rechazan. Un archivo sin cambios (tamaño + fecha) no se reabre.
- El escaneo va en `spawn_blocking`, de uno en uno (`manga::Runtime::scanning`),
  y al abrir la pantalla como mucho uno por minuto; emite
  `manga://library-changed`. Las miniaturas viven en
  `app_cache_dir/manga/thumbs/<volumen>/` y se borran cuando cambia la firma
  del tomo. Una portada propia (`cover.*` o la remota) va en
  `cover-<serie>-<versión>/`: cachear una versión nueva borra las viejas
  (`thumbs::drop_other_covers`) y el reescaneo, la sincronización WebDAV y
  quitar una fuente barren las de series que ya no están
  (`sweep_library_covers`).
- `.mokuro` (`mokuro.rs`) se lee con serde tolerante: campos de más se ignoran,
  de menos toman su valor por defecto. Las páginas se casan por final de ruta,
  luego por nombre sin extensión y, si no casa nada pero el número coincide,
  por orden. `manga_page_text` (la caché del OCR futuro) guarda páginas en ese
  mismo formato y solo rellena las que el `.mokuro` no cubre.
- `manga_sources.auth_json` no sale de Rust: ningún struct de la UI lo lleva
  (igual que la clave de API, en claro en la SQLite y dentro de las copias).
- **Ventana `manga-reader`**: una sola, en `open_aux_window_inner`,
  `capabilities/default.json` y `App.tsx`. Rust guarda a qué tomo apunta
  (`Runtime::reader`); la página lo pide con `manga_reader_target` al cargar o
  lo recibe por `manga://reader-open` si ya existe. Se cierra de verdad (no se
  oculta con "minimizar a la bandeja") y guarda la página en
  `onCloseRequested`; mientras se lee, con debounce de 600 ms.
- Disposición (una, doble, tira), dirección y "portada sola" son por serie;
  ajuste, fondo y capa de texto, globales (`manga::reader::prefs`, en `kv`).
  Las dobles páginas (`buildSpreads`) dejan sola una página ancha (proporción
  > 1,15) y reanudan los pares tras ella. Los tamaños de página salen de
  `fitSpread`, en píxeles exactos, para que la capa de texto case con la imagen.
- La lógica pura del lector está en `reader/layout.ts` y `reader/textLayer.ts`
  con sus tests: navegación por teclado según la dirección (en RTL la flecha
  izquierda avanza), zonas de clic, precarga de ±2 dobles páginas. Las páginas
  de una doble página van en el DOM en orden de lectura y una RTL se pinta con
  `flex-row-reverse`: así Tab y los lectores de pantalla llegan antes a la
  primera.
- **Atajos**: `?` abre la hoja (`ShortcutsSheet.tsx`) en el lector y en la
  pantalla Manga (donde `/` lleva a la búsqueda). Lo que lista sale de
  `screens/manga/shortcuts.ts`, que tiene que decir lo mismo que los
  manejadores (`keyAction`, el `onKey` de `MangaReader.tsx`, `Sidebar`): si
  cambias una tecla, cámbiala en los dos sitios.
- **Teclado y foco**: cada bloque de la capa de texto es un `role="group"`
  enfocable ("texto n de N, página p", `data-block-page` + `data-block`; `[data-page]`
  es solo la página). Tab y las flechas (`blockStep`, `orderBlocks`: por página
  y luego por el orden de mokuro) recorren los bloques en pantalla; pasado el
  último, la flecha pasa la página. Enfocado, Intro busca en el diccionario
  (`openDictionaryForElement`), Ctrl+C y Ctrl+M actúan sobre él y Esc vuelve a
  la página. Un Tab que entra en una barra oculta la saca (solo con
  `:focus-visible`, no con un clic).
- **Rendimiento**: la tira vertical solo monta las páginas a una pantalla por
  encima y dos por debajo (`reader/stripWindow.ts`, como `lineWindow.ts` del
  texthooker), cada una en su offset dentro de un contenedor tan alto como el
  tomo. WebKit no tiene scroll anchoring (y el de Chromium se apaga con
  `overflow-anchor: none` para que los dos se porten igual): la tira recuerda
  qué página está arriba y a qué distancia, y la devuelve ahí cuando cambia la
  altura de las de encima. Pasar página mantiene la doble página anterior hasta
  que la nueva está decodificada (`decode()`, 250 ms como mucho) y
  `PageImage` solo enseña imágenes decodificadas. La tira de miniaturas usa
  `manga://thumb` y `content-visibility`; la rejilla de la biblioteca monta las
  series de 120 en 120 (`GRID_BATCH`).
- **Transiciones**: de una portada de la rejilla a su ficha y de vuelta, con
  View Transitions donde existen (`lib/viewTransition.ts` mira
  `document.startViewTransition`; nunca con `prefers-reduced-motion`) y un
  único `view-transition-name` (`manga-cover`) que `coverTransition.ts` pone a
  mano en la tarjeta justo antes. La espera por la portada nueva va con
  temporizadores: durante la transición no hay `requestAnimationFrame`. Sin
  la API, la ficha entra con `.manga-fade-in`; la ventana del lector funde su
  primera página (`.manga-reader-in`) y otro tomo abierto en ella cruza con
  una transición si la hay.
- **Capa de texto**: cajas escaladas desde `img_width`, `vertical-rl` si el
  bloque es vertical, fuente ajustada a la caja (`blockFontSize`). Estilos en
  `index.css` (`.manga-text-layer`, `data-mode`). El diccionario usa
  `src/hooks/useDictionaryHover.ts`, el mismo que el texthooker
  (`openDictionaryAt` para cualquier elemento). Ctrl+C sin selección selecciona
  el bloque y el evento `copy` escribe sus líneas unidas: no depende del
  permiso de portapapeles de cada webview.
- Los fondos del lector (`.manga-bg-*`) son colores propios en `index.css`, no
  tokens del tema: el papel sigue siendo papel en tema oscuro.
- **Seguimiento** (`manga/session.rs`): el lector solo dice qué tomo enseña
  (`manga_session_open`) y qué páginas hay en pantalla
  (`manga_session_pages`; las dos de una doble página, ninguna en la tarjeta
  final). El resto lo decide Rust en el tick del watcher
  (`session::tick`, sin red): una página cuenta tras
  `manga_page_dwell_secs` (3 s) en pantalla, con `count_japanese` de sus
  bloques, y **una sola vez** (`manga_page_read`, `INSERT OR IGNORE`). Sin
  texto cuenta la página pero no caracteres.
- **Cronómetro** como el de una VN: corre solo con la ventana del lector con
  foco (`Runtime::reader_focused`, de `WindowEvent::Focused` en `lib.rs`) y
  sin inactividad (`afk_timeout_secs`); el tiempo de permanencia de una página
  se para con él. La sesión (`MangaSession`) viaja en `TrackerSnapshot.manga`
  y por `manga://session`; la pintan la barra del lector (`SessionIndicator`)
  y la tarjeta del Dashboard. La sesión no existe hasta el primer tick de
  lectura.
- **Log** al cerrar el lector (`WindowEvent::Destroyed`), al pasar a otro tomo,
  al salir de la app (`RunEvent::Exit`) o tras `manga_idle_log_minutes` (10)
  con el reloj parado; al volver empieza otra sesión. `log_payload` es puro y
  cumple `logRules.ts` (`pages`/`chars`/`time`, `time` en minutos redondeados,
  `description` = "<serie> — <tomo>"); menos de medio minuto sin páginas nuevas
  se descarta. Va a Pendientes (`add_pending`, fuente `manga`) si
  `manga_confirm_before_log` (por defecto) **o si la serie no está vinculada**
  (sin `mediaId`); si no, al outbox. Vincular la serie mientras se lee
  actualiza la sesión (`session::series_linked`). `manga://session-ended`
  dice adónde fue. Los ajustes están en Ajustes → Lector de manga.
- **Líneas**: los bloques de una página que cuenta van a `text_lines` con
  `content_id` = el `mediaId` o `manga:<serie>`, id
  `manga:<tomo>:<página>:<bloque>` (el mismo bloque es siempre la misma
  línea) y `synced = 1`: son contexto local, **nunca** se empujan a la API del
  texthooker. No pasan por `texthooker::ingest`.
- **Minado** (`manga_mine_block`, botón sobre el bloque o Ctrl+M): guarda la
  línea, la mina y recorta en Rust la viñeta alrededor del bloque
  (`crop.rs`: caja escalada de `img_width` a la imagen real, margen del 60 %
  del lado mayor, mínimo 48 px) como PNG en `app_data_dir/mined/` con
  `set_mined_screenshot`. Una búsqueda en el diccionario desde un bloque lleva
  ese id de línea; `mine_dictionary_entry` la crea si la página aún no contó
  (`session::ensure_line`).
- **Discord**: tras la VN y antes del vídeo, misma política de `decide`. La
  lista negra mira el `mediaId` y `manga:<serie>`; una serie sin vincular es
  `Unresolved` (su título es el nombre de una carpeta) y `is_adult` viaja en
  la sesión desde `manga_series.is_adult`. Nunca se publica la portada local.
  Una serie de una fuente de extensión NSFW **o MIXED** se añade con
  `is_adult` y `source_adult`, y vincularla no lo quita
  (`link_manga_series` hace `MAX(?, source_adult)`; el comando relee el
  valor antes de avisar a la sesión).
- **WebDAV** (`manga/webdav/`, M3): mokuro-bunko o cualquier WebDAV. La
  fuente (`manga_sources.kind = 'webdav'`) guarda la URL sin credenciales; el
  `user:pass@` que se escriba en ella pasa a Basic. Las credenciales
  (`auth_json`: anónimo, Basic o Bearer, `webdav::client::Auth`) solo viven en
  Rust: `MangaSource.auth` dice su tipo y nada más, `Auth` no se formatea ni
  en `Debug`, y el formulario de edición manda `null` para conservarlas. Hay un
  test que serializa la biblioteca y la ficha con una contraseña puesta.
- **Listado**: `PROPFIND Depth: 1` (`propfind.rs`, quick-xml por nombre local;
  solo propstats `200`, solo hijos directos de la carpeta pedida, sin `..`; del
  `href` se toma solo la ruta, así que un listado nunca manda a otro host).
  Reglas de `layout.rs`: `mokuro-reader/` en la raíz es la biblioteca; cada
  carpeta, una serie (y la base misma si tiene CBZ); `<Vol>.cbz` + `<Vol>.mokuro`
  + `<Vol>.<img>` portada; `<Vol>.<capa>.mokuro` es una capa alternativa (se
  elige por tomo, `kv:manga:layer:<tomo>`); carpetas de imágenes = tomos. Ids:
  UUID v5 de `fuente + ruta` (`sync::series_id`/`volume_id`).
- **Sincronizar** (`sync.rs`) lista la base siempre y cada serie solo si cambió
  el ETag o la fecha de su carpeta (`manga_series.signature`), de 4 en 4, con
  progreso en `manga://webdav-sync`. Al abrir la pantalla, como mucho cada
  10 min; el botón de cada servidor fuerza la lista completa. Un CBZ cuya firma
  no cambia conserva su índice de páginas. Sin red no se borra nada: la fuente
  queda con `last_error = 'offline'` y lo que se vio por última vez. Cualquier
  petición de `remote.rs` que falle por red la marca así, y la siguiente que
  funcione la limpia (`set_manga_source_status`, que avisa solo si cambia).
- **Lectura** (`remote.rs`), por orden: copia sin conexión
  (`app_data_dir/manga/offline/<tomo>/`, tabla `manga_offline`, nunca se
  desaloja), caché, servidor. Un CBZ remoto se lee con `ranged.rs`: un
  `Read + Seek` por bloques de 512 KB pedidos con `Range` (comprobando
  `Content-Range` y el tamaño listado), así que `archive.rs` funciona igual que
  en disco y solo baja el directorio central y la entrada pedida. El índice
  (`pages_json`) se lee al abrir el tomo (`reader::open`); si el servidor
  responde 200 a un `Range`, se corta la respuesta y el error es
  `needs-download`: la UI lo descarga entero con progreso (`manga://download`)
  a la caché y lo abre. El `.mokuro`, las portadas y las páginas de carpetas
  se bajan enteros a la caché; al abrir la ficha de una serie remota
  (`manga_prepare_series`) se bajan sus `.mokuro`. La miniatura 0 de un tomo
  remoto es su portada sidecar, sin abrir el CBZ.
- **Caché** (`cache.rs`): `app_cache_dir/manga/webdav/`, LRU por fecha de
  modificación (cada lectura la toca), límite `manga_webdav_cache_mb` (2 GB),
  "vaciar" en Ajustes → Lector de manga. Las claves llevan la firma del
  archivo remoto: uno que cambia es otra clave y el viejo envejece solo.
- Todo es bloqueante y va en hilos de `spawn_blocking` (el protocolo, los
  comandos) con `block_on` dentro; nunca en el tick. Formatos de tomo nuevos:
  `remote-archive`, `remote-folder` (`store::is_remote_format`); su
  `source_ref` es la ruta remota, no un archivo. `protocol::page_bytes` y
  `mokuro::volume_text` reciben el `Remote` (`manga::Runtime::remote`).
- **Progreso con Mokuro Reader** (`progress.rs`, opt-in
  `manga_webdav_sync_progress`, porque escribe en el servidor): solo con el
  layout de bunko, en `mokuro-reader/volume-data.json`. Formato verificado en
  Gnathonic/mokuro-reader (`src/lib/settings/volume-data.ts`): claves =
  `volume_uuid` del `.mokuro` (recordado en `kv:manga:uuid:<tomo>`) más las
  secciones reservadas `series` y `tracking`; `progress` es la página desde 1,
  `lastProgressUpdate` decide quién es más nuevo y `deletedOn` es una lápida.
  Al abrir la ficha se trae lo más nuevo; al guardar progreso se escribe
  8 s después del último guardado, cambiando solo `progress`, `completed`
  (nunca a `false`) y `lastProgressUpdate` y conservando todo lo demás, con
  `If-Match` del ETag leído (o `If-None-Match: *` si no existía): si otro
  dispositivo escribió en medio, se relee y se reintenta una vez. Un archivo
  que no es JSON no se toca.
- **OCR integrado** (`manga/ocr/`, M4): detector
  `ogkalu/comic-text-and-bubble-detector` (int8, Apache-2.0; **nunca**
  comic-text-detector, que es GPL-3.0) → recorte por caja → manga-ocr
  (`onnx-community/manga-ocr-base-ONNX` + el `vocab.txt` de
  `kha-white/manga-ocr-base`). Lo corre ONNX Runtime a través de `ort`
  (`=2.0.0-rc.15`, `load-dynamic`, `api-24`): **el runtime nunca se enlaza**;
  se carga de `app_data_dir/ocr/` tras volver a comprobar su SHA-256.
  Variantes: `int8` (por defecto) y `fp16` = encoder fp16 + decoder fp32,
  porque el decoder fp16 de la conversión no carga.
- El preprocesado y la decodificación copian a manga-ocr: gris→RGB, 224×224
  bilineal, `(x/255 − 0,5)/0,5`; beam search con su `generation_config`
  (4 haces, sin repetir trigramas, `length_penalty` 2, `early_stopping`) en
  `beam.rs` (puro, con tests); `text.rs` hace su `post_process` (`…`→`...`,
  `jaconv.h2z` de ASCII, dígitos y kana). El detector (export de RT-DETR)
  recibe la página a 640×640 en `[0,1]` y `orig_target_sizes` y devuelve ya
  cajas en píxeles. `layout.rs` (puro): clases 1 y 2 con `score ≥ 0,4`,
  duplicados fuera, vertical si la caja es más alta que ancha, orden RTL
  (filas que comparten ≥25 % de altura, de derecha a izquierda) y el texto
  repartido en columnas/líneas según la caja.
- El resultado es una página `.mokuro` en `manga_page_text` con engine
  `builtin`; `mokuro::volume_text` solo la usa donde el `.mokuro` no tiene
  página, y descarta texto cuyo `img_path` ya no es el de esa página. Una
  corrección a mano es `manual` y **gana** también al `.mokuro`; el OCR no la
  rehace nunca. `ocr_state` pasa a `ocr` cuando un tomo sin texto recibe la
  primera página; `MangaVolume.ocr_pages` cuenta las páginas guardadas.
- **Hilo propio** (`worker.rs`): cola con prioridad caja dibujada > páginas
  en pantalla > las siguientes > "Leer texto" del tomo, sin duplicados. El
  motor se carga con el primer trabajo y se suelta tras 5 min sin nada.
  Nunca en el tick ni en un worker de tokio. Execution providers en
  `platform` (`ocr_execution_providers`: DirectML en Windows, CoreML en
  macOS, nada en Linux): si el acelerador no arranca o falla una página,
  todo se reconstruye en CPU y se reintenta. En Windows `DirectML.dll` se
  precarga desde `ocr/` (`ocr_prepare_runtime`): la de System32 es vieja.
- Una página contada sin texto (el OCR llegó después) recibe sus caracteres
  y líneas una vez (`session::page_text_changed`, `fill_manga_page_chars`).
- **Descarga solo con consentimiento** (`download.rs`): el diálogo
  (`OcrInstallModal`) enseña qué se baja, licencias, tamaño, espacio libre
  (`sysinfo::Disks`) y carpeta. Cada archivo va a `.part`, se reanuda con
  `Range` y solo toma su nombre si tamaño y SHA-256 (fijados en `assets.rs`)
  coinciden; si no, se borra. ONNX Runtime 1.24.4 sale de los paquetes
  oficiales de Microsoft en nuget.org (`Microsoft.ML.OnnxRuntime`,
  `…DirectML` + `DirectML.dll` de `Microsoft.AI.DirectML` 1.15.4): se lee
  solo la entrada de este SO por rangos (cabecera local del zip + datos
  deflate). Sin build para Mac Intel (`supported = false`). "Eliminar
  modelos" borra todo; lo que siga cargado (Windows) se borra al arrancar
  (`.delete-pending`). Los modelos no van en la copia de seguridad.
- **UX**: cada tomo dice su texto (mokuro, listo, n de N, sin texto) y
  "Leer texto" lo procesa con "Leyendo x/y", cancelable, y notificación al
  acabar (`i18n::ocr_done_*`). El lector pide al vuelo las páginas en
  pantalla sin texto y las siguientes (`manga_ocr_auto`). Corrección a mano
  con E (`EditLayer`, `BlockEditor`): editar texto y dirección, volver a
  leer, borrar, y arrastrar para dibujar una caja, que el OCR lee al momento
  (`manga_ocr_region`). Se guarda por página (`manga_save_page_text`).
- **Exportar** (`export.rs`): `.mokuro` con todas las páginas (mokuro + OCR
  + correcciones; vacías con su tamaño si es local), junto al tomo local
  (`Vol.cbz`/`Vol/` → `Vol.mokuro`, pasa a ser su `.mokuro`) o subido junto
  al tomo WebDAV tras confirmar, con `If-None-Match: *` o `If-Match`; sin
  permiso de escritura el error es `read-only`. Conserva título y uuids de un
  `.mokuro` previo y recuerda el `volume_uuid` para el progreso de Mokuro
  Reader. Reemplazar exige una segunda confirmación.
- **owocr como motor** (`ocr/owocr.rs`, M5; Ajustes → Lector de manga →
  Motor de OCR: Integrado · owocr, `manga_ocr_engine`, `manga_owocr_port`).
  owocr (AuroraWright/owocr, GPL-3.0) es un programa que el usuario instala
  y arranca: la app **no** lo instala, no lo empaqueta ni copia su código;
  solo le habla por WebSocket, siempre a `127.0.0.1` (owocr escucha en
  `0.0.0.0`, pero las páginas nunca van a otra máquina). Protocolo
  verificado en owocr 1.26.8 (`run.py`: `WebsocketServerThread`,
  `OutputResult`; `ocr.py`: `OcrResult`): con `read_from=websocket
  write_to=websocket output_format=json` (puerto 7331), un mensaje binario
  es una imagen; contesta `True` al que la mandó (`False` en pausa) y el
  resultado, `json.dumps(asdict(OcrResult))`, va a **todos** los clientes,
  sin id. Si el motor falla no manda nada. Con `output_format=json` owocr
  se salta los motores sin coordenadas (`mangaocr`; `mangaocrs` sí las da);
  en modo texto el resultado es texto plano.
- Por eso **una petición en vuelo cada vez**: todo pasa por la cola del
  worker (también "Probar conexión", `Work::Probe`), se vacía lo que llegó
  antes de mandar, un resultado solo cuenta tras nuestro `True` y si su
  `image_properties` mide lo que nuestra imagen; lo demás se descarta.
  Timeout (60 s la página, 20 s la prueba) y, tras cualquier error, la
  conexión se tira para que un resultado tardío no caiga en la siguiente.
  Errores como códigos: `owocr-unavailable`, `-paused`, `-timeout`,
  `-no-coordinates` (llegó texto), `-bad-reply`. Salvo el timeout, vacían
  la cola con el error en `OcrStatus.error`, que la UI avisa una vez
  (`useOcrStatus`). **Nunca se cae al integrado** si owocr falla: el motor
  de cada trabajo es el de los ajustes al empezarlo (`choice`, con
  `blocking_read`, no un valor por defecto).
- Conversión (`to_page`, pura): un párrafo = un bloque, cada línea con sus
  cuatro esquinas en píxeles (la rotación se deshace en píxeles, como owocr
  la construye en `quad_to_bounding_box`), dirección del párrafo, de sus
  líneas o de su forma, columnas de derecha a izquierda, el orden de
  `layout::reading_order` y el `post_process` de manga-ocr. Se guarda con
  engine `owocr`; las correcciones (`manual`) siguen ganando. La caja
  dibujada va recortada con margen (su detector pierde lo que toca el
  borde) y solo cuentan las líneas con el centro dentro (`to_text`).
- `OcrStatus` lleva `engine`, `ready` (owocr, o el integrado instalado) y
  `usable`: la UI usa esos dos, no `installed`/`supported`, para pedir
  páginas. Con owocr no hay diálogo de descarga.
- Probar el OCR de verdad: los tests `#[ignore]`
  `ocr::download::…downloads_the_real_files` (`NTDC_OCR_FETCH_DIR`),
  `ocr::engine::…reads_a_real_page` (`NTDC_OCR_DIR`, `NTDC_OCR_PAGE`,
  `NTDC_OCR_EXPECT`) y, con un owocr arrancado a mano,
  `ocr::owocr::…reads_a_real_page_with_owocr` (`NTDC_OWOCR_PORT`,
  `NTDC_OCR_PAGE`, `NTDC_OCR_EXPECT`). Nunca guardes modelos ni runtime en
  el repo.
- **Extensiones** (`manga/suwayomi/`, M6): fuentes de Tachiyomi/Mihon
  (tienda Keiyoushi) a través de Suwayomi-Server, en dos modos
  (`manga_ext_mode`): **gestionado** o **URL propia**. Cada modo es una fila
  de `manga_sources` (`suwayomi-managed`, `suwayomi-custom`, `kind =
  'suwayomi'`) con sus credenciales en `auth_json` (Basic, como WebDAV);
  `ExtStatus` no lleva URL, puerto ni credenciales (hay un test). Verificado
  contra **v2.4.2366** (introspección del esquema, `ConfigModule.kt`,
  `ApplicationRootDir.kt`, `ExtensionStoreMutation.kt`, `JavalinSetup.kt`).
- **Gestionado**: nada se descarga ni arranca antes del diálogo
  (`ExtInstallModal`: qué, tamaño, espacio, carpeta, licencias, que las
  extensiones son código de terceros). `install.rs` baja el jar y el JRE de
  Temurin de este SO/arquitectura (`platform::java_runtime_target`) con
  `ocr::download::fetch` (reanudable, SHA-256 fijado en `assets.rs`, el del
  JRE es el que publica Adoptium) y desempaqueta el JRE (zip en Windows,
  tar.gz si no) por una carpeta temporal, rechazando rutas que salgan; `.ready`
  lleva el checksum. Todo en `app_data_dir/suwayomi/` (`bin/`, `jre/`,
  `data/` = `rootDir`, `server.log`). Actualizar la versión es un PR que
  cambia `assets.rs`.
- **Arranque** (`process.rs`): `-Dsuwayomi.tachidesk.config.server.<clave>`
  manda sobre su `server.conf` (y `rootDir` solo se lee así): `ip=127.0.0.1`,
  puerto libre al azar, `systemTrayEnabled`, `initialOpenInBrowserEnabled`,
  `webUIEnabled` y `kcefEnabled` (sin Chromium) a `false`,
  `authMode=BASIC_AUTH` con usuario y contraseña aleatorios,
  `globalUpdateInterval=0`, `backupInterval=0`, `-Djava.awt.headless=true`,
  `java.io.tmpdir` dentro de `data/`. Los argumentos van en un `@archivo`
  (0600, se borra al responder) para que la contraseña no salga en la lista
  de procesos; Suwayomi no la escribe en su `server.conf`. Sin ventana en
  Windows (`platform::hide_child_window`, `CREATE_NO_WINDOW`). Espera a
  `aboutServer` (hasta 180 s); un servidor que dejó un cierre brusco
  (`server.pid`) se para antes. Se arranca solo al usar extensiones
  (`Service::connect`, bajo un `tokio::Mutex`), se para tras
  `manga_ext_idle_minutes` sin uso (hilo `suwayomi-idle`; servir una página
  cuenta como uso), al cambiar de modo y en `RunEvent::Exit`.
- **Tienda**: al arrancar el gestionado se añade Keiyoushi
  (`addExtensionStore` con el `index.pb` que nombra su `repo.json`, y el de
  `raw.githubusercontent.com` si GitHub no responde); a un servidor propio
  solo si el usuario pulsa el botón. Los errores GraphQL llegan con 200 y una
  traza en `errors[].message`: se registran y son `SwError::Server`, nunca
  se enseñan.
- **Lo que ve la UI** (`catalog.rs`): NSFW y MIXED se filtran **en Rust**
  salvo `manga_ext_show_adult` (por defecto no), también al instalar,
  explorar, abrir o añadir (`adult-hidden`). Imágenes por token opaco
  (`manga://…/ext/<token>`, `Service::image_token`, uuid v5 de fila + ruta);
  Rust solo pide rutas `/api/v1/…` del mismo servidor (`graphql::server_path`,
  también para las páginas) y sin redirecciones.
- **Biblioteca** (`library.rs`): serie = `manga_series` (`source =
  'suwayomi'`, `source_ref` = id de Suwayomi), capítulo = volumen
  `suwayomi-chapter` (`store::is_served_format`; `is_remote_format` sigue
  siendo solo WebDAV, así que el progreso de Mokuro Reader y la exportación
  `.mokuro` no los tocan). Ids: uuid v5 de fila + id. Las rutas de página
  (`fetchChapterPages`) se guardan en `pages_json` al abrir el capítulo
  (`Remote::ensure_index`); las páginas salen de la copia sin conexión, la
  caché de WebDAV o el servidor (`Remote::chapter_page`). "Leer" un capítulo
  añade la serie (la sesión necesita una para el log). Sesión, logs, minado
  y OCR funcionan igual; el texto OCR se guarda por capítulo. "Descargar"
  guarda las páginas en `manga/offline/<capítulo>/` (sin depender de que el
  servidor siga, a diferencia de la descarga propia de Suwayomi).
- Probar Suwayomi de verdad: `suwayomi::tests::runs_the_real_server`
  (`#[ignore]`; `NTDC_SWY_DIR`, `NTDC_SWY_JAR`, `NTDC_SWY_JRE`,
  `NTDC_SWY_EXT`, `NTDC_SWY_QUERY`). Nunca guardes el jar, el JRE ni sus
  datos en el repo.
- **Amigos inmersos** (`share_activity`, `sync::immersion_presence`, pura y
  con tests): el manga va tras la VN y antes del vídeo, con la misma política
  que Discord (`discord::manga_privacy`): lista negra = nada, adulta o sin
  vincular = solo "Inmersión en japonés". La lista negra se aplica aunque
  Discord esté apagado.
- Un servidor que rechaza las credenciales (`last_error = 'unauthorized'`)
  cuenta como uno caído en la ficha (`unreachableReason`): "Leer" solo ofrece
  lo descargado y el aviso dice por qué. Los contadores pasan por
  `formatCount` o `{{count, number}}`, con el separador de miles del idioma.

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
`discord://status`, `updater://available`, `updater://progress`,
`manga://library-changed`, `manga://reader-open`, `manga://session`,
`manga://session-ended`, `manga://webdav-sync`, `manga://download`,
`manga://ocr-status`, `manga://ocr-page`, `manga://ocr-progress`,
`manga://ext-status`. Están
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
- **Fushi** (Flutter + libmpv embebido) titula su ventana siempre "Fushi" y no
  lee `mpv.conf`, así que `fushi.rs` lee su `fushi.db` en solo lectura mientras
  el proceso `fushi` corre: `video_books` (`video_path`, `last_position_ms`,
  `last_played_at`, que escribe cada segundo de reproducción) y la preferencia
  `video_duration_ms_<bookUid>` (codificada `i:<ms>`). Lo presenta como un
  `MediaPlayer` del bus de medios. Sin escritura en 5 s = pausa; un Fushi en
  pausa mantiene una sesión pero nunca la abre. La base está en
  `%APPDATA%\Fushi\Fushi` (macOS `app.fushi.reader`) o, si el usuario movió los
  datos, en `<flutter.data_root>/support` según su `shared_preferences.json`.
  Es su esquema interno, no una API: si cambia, la consulta falla en silencio.
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
