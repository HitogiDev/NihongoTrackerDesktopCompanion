# AGENTS.md

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
pwsh -File scripts/test-reading-overlay-runtime.ps1 # regresión nativa del overlay en Windows
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
  playback.rs    Posición real: IPC de mpv y interfaz web de MPC
  watcher.rs     Bucle de detección; convierte sesiones en logs
  texthooker.rs  Cliente WebSocket Textractor/Agent + lector de portapapeles
  api.rs         Cliente REST (x-api-key)
  connection.rs  Comprobación de cuenta, usuario y estado compartido entre ventanas
  streak.rs      Aviso de "racha en riesgo": day-keys, ventana y antirruido
  discord.rs     Rich Presence opcional: política de privacidad + hilo de IPC
  mining.rs      Frases guardadas desde líneas del texthooker
  i18n.rs        Los pocos textos que muestra Rust (bandeja y notificaciones)
  sync.rs        Outbox con reintentos + push de líneas
  commands.rs    Superficie IPC
packages/
  yomitan-content/  Renderer de structured content de Yomitan (TS), compartido
                    con el Frontend web. Sin Tauri dentro
src/
  i18n/          i18next: `locales/{en,es}/*.json`, un namespace por pantalla
  lib/ipc.ts     Único punto de entrada a los comandos Rust, tipado
  lib/types.ts   Espejo de los tipos de Rust — hay que mantenerlos sincronizados
  store/         zustand: settings, tracker, hooker (se alimentan de eventos)
  screens/       Dashboard, Queue, Library, Texthooker, Mining, Settings, Onboarding
  components/ui/ Design system copiado de NihongoTracker/Frontend
```

### Flujo de datos

1. `watcher.rs` sondea cada `poll_interval_ms` y publica un `TrackerSnapshot`
   por el evento `tracker://state`.
   El indicador de ventana activa excluye el PID del companion, incluidas sus
   ventanas auxiliares. La observación de foco conserva ese PID para las sesiones.
2. El frontend nunca hace polling: escucha eventos y llama comandos.
   `connection.rs` comprueba la API al iniciar, al cambiar servidor o clave y
   cada 15 s si `auto_reconnect` está activo. Emite `connection://changed` a
   todas las ventanas y guarda el nombre de usuario devuelto por `users/me`.
3. Los logs terminados van al **outbox** de SQLite, nunca directos a la red.
   `sync.rs` los drena cada 30 s y al confirmar desde la UI.

La conexión automática del WebSocket del texthooker empieza al detectar una VN
o abrir el lector integrado. No conecta al iniciar la aplicación sin lectura.
Cerrar el juego desconecta el socket de su obra. Cerrar el lector sin una VN
activa también detiene los reintentos.

### Quién manda en cada cosa

- La sesión del texthooker (`content_id`) la fija **Rust** al detectar una VN, no
  el frontend: la ventana puede estar en la bandeja y su listener no dispararse.
- El cronómetro de una VN detectada lo lleva el watcher; el temporizador manual
  de la pantalla del texthooker solo se usa cuando no hay sesión automática.
  `hooker_set_timer` aplica ediciones explícitas al detector cuando recibe
  `update_detected_session`. Una edición pendiente por PID, obra e inicio evita
  que un tick en curso restaure el tiempo anterior. El guardado periódico manual
  no edita sesiones detectadas. Reiniciar pausa la sesión y conserva las líneas.
- El XP nunca se calcula en el cliente: se pide a `POST /api/logs/preview-xp`.
- El día del usuario lo decide `streak.rs`, no el frontend ni el reloj del SO.

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
- La notificación es accionable sólo en Windows: el plugin de Tauri no expone el
  clic en escritorio, así que ahí el toast se construye con
  `tauri-winrt-notification` (botón + `on_activated`), con el AppUserModelID de
  la app. `notifications.rs` registra su nombre e icono en HKCU al iniciar y
  crea un acceso directo del menú Inicio con el mismo AppUserModelID. Windows
  necesita ese acceso para resolver el nombre del emisor, también en dev y en
  ejecutables sin instalar. No uses la identidad de PowerShell.
  En el resto de sistemas cae al plugin,
  sin botón. El clic (y la entrada de la bandeja) emiten `streak://open-log`.

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

### Ventanas auxiliares

- `commands::open_aux_window` crea una única ventana `settings`, `texthooker` o
  `reading-overlay-settings` y enfoca la existente. `App.tsx` elige la pantalla por el
  label de Tauri; cada webview carga su propio store de frontend, pero ambos
  usan el mismo `AppState` y SQLite de Rust.
- Cerrar Settings o Texthooker solo oculta la ventana. Así conserva su sesión,
  geometría y estado mientras la app siga abierta. Añadir un label nuevo requiere
  incluirlo en `src-tauri/capabilities/default.json`.
- Las ventanas de ajustes tienen al popup como propietario para permanecer
  encima del diccionario. Comparten la opción `always_on_top` del overlay.
- Los eventos globales que muestran formularios o cambian la pantalla principal
  solo deben escucharse en `main`; escucharlos en cada webview auxiliar duplica
  modales o abre varias ventanas.

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
2. Portada adulta en VN (`isAdultImage`), `isAdult` en otros tipos, media sin resolver, o el modo genérico global → presencia genérica
   ("Inmersión en japonés"), sin título ni portada. El progreso (episodio o
   caracteres) sí se queda: no identifica nada.
3. Si no, la presencia completa.

- `isAdult` viaja con los datos, no se consulta al vuelo: `MediaMatch.is_adult`
  y la columna `vn_apps.is_adult` (migración en `db::MIGRATIONS`, porque SQLite
  no tiene `ADD COLUMN IF NOT EXISTS`). Si añades otra ruta que cree un
  `MediaMatch` o un `VnApp`, arrastra el flag o el título acabará en Discord.
- Las VN usan `isAdultImage` para Discord, igual que el blur de la web.
  `vn_apps.is_adult_image` conserva el dato para uso sin conexión. Un valor NULL
  identifica registros antiguos. Hasta obtener el dato se conserva el criterio anterior.
  La consulta se hace fuera del watcher al iniciar la VN, activar Discord o recuperar
  conexión. Las respuestas de otro servidor se descartan.
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

### Eventos Rust → frontend

`tracker://state`, `vn://started`, `vn://ended`, `texthooker://line`,
`texthooker://status`, `texthooker://timer`, `outbox://changed`,
`pending://changed`, `settings://changed`, `log://celebration`,
`streak://reminder`, `streak://open-log`, `streak://auth-required`,
`discord://status`. Están
centralizados en `src/lib/ipc.ts` (`events`).

`log://celebration` lleva el `celebration` y los `newAchievements` que devuelve
`POST /api/logs`; lo emite `sync.rs` al entregar del outbox, así que un log que
se encoló sin conexión celebra igual cuando por fin sale.

### Juegos, minería y panel flotante

- El registro `vn_apps` también admite `media_type: game`. El watcher cuenta un
  juego solo en primer plano, respeta AFK y pausa manual, no abre el texthooker y
  deja su log en Pendientes. `detect_games` lo controla por separado.
- LunaHook comunica los hooks detectados y sus muestras al estado del watcher.
  El watcher solicita `with_exe(UpdateKind::OnlyIfNotSet)` a sysinfo para obtener
  la ruta del ejecutable y elegir el helper de la arquitectura correcta.
  El botón de opciones de LunaHook abre el selector aunque no haya candidatos.
  `selection_key` distingue el código y el contexto split. TYPEMOON incluye también
  el contexto de llamada. `vn_apps.native_hook_code` guarda esa clave por ejecutable.
  Al iniciar el helper, envía el código sin los contextos en `--hook`, codificado
  en base64 UTF-8. El helper lo restaura con `Luna_InsertHookCode` después de
  recibir la conexión real de LunaHost, sin esperar al primer diálogo.
  Las muestras se actualizan antes del filtro. Solo la fuente seleccionada entra
  en el texthooker. Las selecciones antiguas por código siguen siendo compatibles.
  El target de LunaHook lleva su `content_id`. Cambiar la sesión visible no cambia
  el destino de las líneas nativas ni inicia el cronómetro de otra sesión.
  El socket externo se desconecta al iniciar LunaHook y no pausa su sesión nativa.
  Un join compacto permite usar WebSocket sin borrar el hook guardado.
  El botón de opciones del hook solo aparece cuando se selecciona LunaHook.
  El comando de cambio devuelve el estado confirmado. Una espera limitada evita
  que una confirmación perdida bloquee los controles.
  Esta elección queda atada al PID y la obra hasta cerrar o reiniciar el juego.
  LunaHook mantiene las muestras, pero no ingresa líneas mientras se usa WebSocket.
- `vn_apps.reader_mode` elige `integrated`, `browser` u `overlay`.
  La migración conserva el switch `browser_texthooker` y las URLs anteriores.
  Una apertura automática en modo Overlay no abre el lector completo.
  Una apertura manual usa el lector integrado para una VN en modo Overlay.
  `open_work_texthooker` respeta esta opción desde la sesión, Sidebar y el overlay.
  El evento de inicio pasa la sesión detectada al comando antes de que el watcher
  publique su nuevo snapshot. Esto evita una carrera en la apertura automática.
  Sin una sesión de juego activa, abre el lector integrado.
  Las URLs extra antiguas activan el switch una vez durante la migración.
  `native_bridge.rs` publica líneas y hooks por un WebSocket en loopback, con
  token aleatorio y validación del origen del servidor configurado. El token viaja
  en el fragmento `ntdc` del lector web. Cada conexión queda atada a su obra.
  El navegador puede guardar la selección por el bridge. Conserva los IDs de las
  líneas para evitar duplicados al sincronizar. Ambos lectores desactivan el botón
  del WebSocket externo mientras usan LunaHook.
- `text_cache.rs` conserva portada, caracteres totales, progreso e historial web
  en SQLite, con una clave por servidor, credencial y obra. La portada se guarda
  como imagen base64 para usarla offline. Los datos se actualizan al abrir las
  estadísticas, al reconectar y después de sincronizar. `texthooker://context`
  invalida los datos de la vista. El historial combina filas web y locales.
- `mined_sentences` guarda una copia de la línea y su obra, independiente de si
  se borra la línea original. `mining.rs` centraliza la captura manual y el atajo
  global Ctrl+Shift+M. La exportación local a AnkiConnect vive en `sync.rs`.
- El webview `overlay` comparte el estado de Rust con `main`, pero solo renderiza
  `SessionOverlay`. Su visibilidad se guarda en `overlay_enabled`. Los controles
  de registro envían `overlay://navigate` a la ventana principal; el control del
  texthooker abre su ventana auxiliar dedicada.

### Overlay de lectura

- `reading_overlay.rs` controla las ventanas `reading-overlay` y
  `reading-overlay-popup`, sin activarlas ni quitar el foco al juego.
  Ambas desactivan la sombra nativa para evitar el borde blanco de Windows.
  El popup tiene al overlay como propietario y queda siempre encima de él.
  El watcher fija su PID, obra e inicio de sesión antes de arrancar LunaHook.
  `texthooker::ingest` entrega únicamente líneas aceptadas a este overlay.
- `reading-overlay://state` alimenta las vistas. El popup mantiene la línea
  consultada mientras el overlay conserva la última línea recibida.
  Una sesión nueva descarta popup, texto y paso de clics de la sesión anterior.
- El overlay y el lector integrado pueden estar abiertos juntos. Los botones
  del overlay y del lector abren la otra vista sin cambiar la sesión ni la entrada.
  El overlay sigue visible al enfocar el lector si ambos muestran la misma obra.
  Ocultarlo manualmente o cerrar la VN conserva las reglas de visibilidad.
  `hide_when_unfocused` permite mantenerlo visible al cambiar de ventana.
  Mostrarlo manualmente ignora el foco hasta volver a la VN. Cambiar el título
  de la misma sesión no descarta su visibilidad, texto ni popup.
  El overlay vacío muestra su estado aunque la opacidad en reposo sea cero.
  Mostrarlo manualmente revela sus controles durante cinco segundos.
  Los contadores de la barra del lector usan las líneas locales. Los totales
  del servidor se muestran solo en las estadísticas históricas.
- Los ajustes del lector se abren en `reading-overlay-settings`. El overlay
  sigue visible mientras esa ventana tiene el foco y la VN continúa activa.
- Consulta el foco y la geometría de ventanas antes de bloquear el estado del overlay.
  Las consultas de Tauri esperan al hilo de eventos, cuyos callbacks usan ese estado.
  Esto evita una espera circular al mover o redimensionar el overlay o su popup.
  La prueba nativa usa ventanas aisladas y SQLite en memoria. El script añade
  el manifiesto de Common Controls al ejecutable de prueba y limita su duración.
- La geometría de ambas ventanas vive en SQLite `kv`. Los ajustes visuales
  y atajos viven en `Settings.reading_overlay`. El contorno y la sombra del texto
  se configuran ahí y usan `readingOverlayTextStyle` en el overlay y la vista previa.
  Rust valida colores hexadecimales y limita grosor, desenfoque e intensidad. Los conflictos de atajos se
  exponen en el estado del overlay. No registres el atajo de minería de nuevo.
- `DictionaryPopup` admite modo ventana y contexto de minería por línea y obra.
  El popup del overlay tiene opacidad independiente, con fondo opaco por defecto.
  Sus ajustes incluyen habilitación, espera de lookup y espera de cierre.
  Cada nueva búsqueda por hover reposiciona el popup junto al cursor, cambia de
  lado si falta espacio y respeta el área de trabajo y la escala del monitor.
  `textHitAtPoint` comprueba el rectángulo de cada carácter, no solo la posición
  de inserción del navegador. Conserva los offsets UTF-16 de la consulta.
  El popup aparece a un lado del texto vertical y arriba o debajo del horizontal.
  El overlay envía los límites del carácter para que su popup nativo lo deje libre.
  La barra de título completa arrastra el overlay, excepto sus botones.
  El paso de clics solo afecta al cuerpo. Rust observa el cursor cada 16 ms mientras
  está activo y conserva la interacción en los 32 px lógicos de la barra.
- El lector integrado pide confirmación antes de borrar todas las líneas.
  La confirmación queda atada a la obra visible y se cierra al cambiar de sesión.
  Rust obtiene la frase y el título originales desde SQLite. No uses la sesión
  visible del lector para minar una consulta del overlay.
- `NativeHookChoices` comparte el selector entre lector y popup. El guardado
  desde el overlay valida PID, obra e inicio de sesión antes de cambiar el hook.
- El frontend de estas ventanas necesita fondo transparente también durante
  el arranque. Incluye ambos labels en las capacidades de Tauri.

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
- **Ningún texto visible se escribe a pelo.** Todo pasa por `useTranslation` y
  vive en `src/i18n/locales/en` y `.../es`; las dos carpetas tienen que tener
  exactamente las mismas claves. Inglés es el idioma de respaldo.
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
- La posición real manda sobre el tiempo transcurrido: `AnimeSession::ready`
  usa `position/duration` cuando el reproductor la da y solo cae al reloj si no.
  Un reproductor nuevo con API se añade a `playback::probe`, no al watcher.
- El sondeo de posición sale del tick (`request_playback`) con un `AtomicBool`
  que impide que se acumulen sondas si el reproductor tarda.
- El conteo de caracteres debe seguir siendo solo kana + kanji
  (`texthooker::count_japanese`), igual que `countJapaneseCharacters` en la web,
  o las estadísticas dejarán de cuadrar con el sitio.
- Las dos entradas del texthooker (WebSocket y portapapeles) terminan en
  `texthooker::ingest`. Cualquier filtro nuevo va ahí, no duplicado en cada
  camino. El del portapapeles además exige que la copia lleve japonés, o cada
  Ctrl+C acabaría siendo una línea.
- Los filtros de `Settings.native_hook_text_filters` se aplican solo a LunaHook
  en `texthooker::ingest`, antes de guardar, contar y publicar.
  `text_filters.rs` compila y conserva las regex hasta que cambian las reglas.
  Las regex eliminan coincidencias en orden, seguidas de las palabras literales.
  Las muestras del selector de hooks conservan el texto original.
  `preview_native_hook_filters` usa el mismo motor. `save_settings` rechaza regex inválidas.
- `texthooker_prevent_duplicates` remueve frases repetidas dentro de cada línea con
  `(.{2,}?)\1`, una pasada global. Solo reemplaza fragmentos con al menos dos
  caracteres alfanuméricos distintos. Conserva sonidos y puntuación como `ああ……`.
  Se aplica a todas las fuentes antes de
  las reglas de LunaHook y del conteo. La vista previa incluye esta opción.
  No descarta una frase por haber aparecido en una línea anterior.
- `src/screens/Texthooker.tsx` es un port de `HookerScreen.tsx` de la web: mismo
  layout, mismas clases (`th-root`, `th-topbar`, `th-line-item`…), mismas
  opciones de fuente y tema, mismo cronómetro anclado a reloj de pared. Si
  cambias algo ahí, compáralo con el original antes.
- El lector sigue las líneas nuevas solo cuando está en la última. El scroll
  vertical RTL avanza hacia valores negativos. Conserva la posición manual.
- El estado del cronómetro vive en los dos lados: la UI lo lleva y lo replica en
  `AppState::hooker_timer_active` con `hooker_set_timer_active`, porque la
  ingestión en Rust necesita saberlo para los ajustes de "en pausa" y de
  arranque automático.
