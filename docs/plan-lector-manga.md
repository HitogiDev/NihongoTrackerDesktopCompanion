# Plan: lector de manga (local + mokuro, WebDAV, extensiones vía Suwayomi, OCR)

## Contexto

El plan de rendimiento y UI/UX ya está hecho y publicado (v0.2.0). Ahora el
autor quiere un **lector de manga** dentro del companion para leer en japonés y
que cuente para NihongoTracker:

- **Local**: carpetas de imágenes, CBZ/ZIP y volúmenes procesados con mokuro.
- **WebDAV**: servidores tipo mokuro-bunko (lo que corre mokuro.moe), con su
  layout `mokuro-reader/<Serie>/<Vol>.cbz + <Vol>.mokuro + <Vol>.webp`.
- **Extensiones de Tachiyomi/Mihon** (tienda Keiyoushi) a través de
  **Suwayomi-Server**, gestionado por la app o en una URL propia, avisando al
  usuario antes de descargarlo.
- **OCR** de páginas sin mokuro: motor **integrado en Rust (ONNX)** por defecto;
  **owocr** como motor alternativo si el usuario lo tiene.
- Cuenta caracteres, páginas y tiempo; diccionario al pasar el ratón; minado a
  Anki con recorte de la viñeta. **Muy estético, con buena UI y UX.**

Este plan sustituye al anterior en este archivo; el de rendimiento/UI vive en
`docs/plan-rendimiento-ui-ux.md` y está completado.

## Hechos verificados que condicionan el diseño

- **`.mokuro`** (kha-white/mokuro, GPL-3.0; solo leemos su formato):
  `{version, title, title_uuid, volume, volume_uuid, pages[]}`; página
  `{img_path, img_width, img_height, blocks[]}`; bloque
  `{box:[x1,y1,x2,y2], vertical, font_size, lines[], lines_coords[]}`.
  mokuro-bunko añade campos (`ocr_engine`…): **tolerar campos desconocidos**.
  En bunko `img_path` apunta dentro del CBZ.
- **mokuro-bunko / mokuro.moe**: WebDAV (`DAV: 1,2`), PROPFIND anónimo
  permitido por defecto; Basic o Bearer si hay login. Cada volumen: `.cbz`
  (~600 MB), `.mokuro` (~0,5 MB), `.webp` (portada). Capas extra
  `<Vol>.<nombre>.mokuro`.
- **Keiyoushi**: `repo.json` → `index_v2 = …/raw/repo/index.pb` (protobuf gzip,
  esquema de Mihon `NetworkExtensionStore`). 1.710 extensiones; el campo 501
  trae un `.jar` ya convertido que Suwayomi usa directamente. NSFW es un enum
  (SAFE/MIXED/NSFW); el idioma va por fuente (198 extensiones con `ja`).
- **Suwayomi-Server** v2.4.x, MPL-2.0. El `.jar` pesa 183 MB y exige JRE 21+;
  los paquetes con JRE pesan 330–345 MB (llevan Electron). API GraphQL en
  `POST /api/graphql` (`addExtensionStore(indexUrl)`, `fetchExtensions`,
  `updateExtension`, `fetchSourceManga`, `fetchChapters`, `fetchChapterPages`);
  REST solo binarios (`/api/v1/manga/{id}/chapter/{i}/page/{n}`, iconos,
  miniaturas). Se arranca headless con
  `-Dsuwayomi.tachidesk.config.server.<clave>=…` (`port`,
  `systemTrayEnabled=false`, `initialOpenInBrowserEnabled=false`,
  `webUIEnabled=false`, `authMode=basic_auth`, `rootDir`).
- **Modelos ONNX con licencia compatible**: manga-ocr
  `onnx-community/manga-ocr-base-ONNX` (Apache-2.0; encoder int8 87 MB +
  decoder 30 MB, o fp16 172 + 59 MB). Detector: **no** comic-text-detector
  (GPL-3.0); usar `ogkalu/comic-text-and-bubble-detector` (Apache-2.0, int8
  44 MB). Crate `ort` 2.0.0-rc con `load-dynamic`: el runtime de ONNX se
  descarga al activar el OCR, no va en el instalador.
- **owocr** (Python, GPL-3.0, proceso aparte): con `output_format=json` da
  párrafos/líneas con cajas normalizadas 0–1 y dirección de escritura (todos los
  motores salvo MangaOcr sin segmentar). Entrada/salida por WebSocket (puerto
  7331): sin id de petición y con broadcast, así que se usa **una petición
  cada vez**, en serie.
- **Código reutilizable** (ver `CLAUDE.md`):
  - `texthooker::count_japanese`.
  - `db.insert_line` / `TextLine` (necesario para minar con contexto).
  - `commands::log_text_session` como plantilla del log.
  - `sync::add_pending` / `enqueue_log`.
  - `DictionaryPopup` + `textHitAtPoint` (`src/lib/textHit.ts`), que funcionan
    sobre texto posicionado encima de una imagen.
  - Minado: `db.set_mined_screenshot` acepta cualquier PNG en
    `app_data_dir/mined/`.
  - `MediaSearchModal` con tipo `manga`.
  - Eventos de progreso como `updater://progress`.
  - Design system: `Page`, `Modal`, `Toaster`, `EmptyState`, `ErrorState`,
    `ListSkeleton` y las superficies `surface*`.
- **Lo que falta hoy**: el webview no puede cargar archivos locales (sin asset
  protocol, `csp: null`); el crate `zip` solo está en dev-dependencies; no hay
  protobuf.

## Reglas para quien lo ejecute

- Leer `CLAUDE.md` entero. Sus reglas mandan:
  - i18n en `en` y `es` con las mismas claves.
  - Nunca interpolar clases.
  - Comando nuevo = `commands.rs` → `lib.rs` → `ipc.ts`.
  - Structs para la UI con `ts_rs` y luego `npm run bindings`.
  - Código de SO solo en `platform/`.
  - Nada de red dentro del tick del watcher.
  - Toda petición a NihongoTracker pasa por `sync.rs`.
- Rama nueva desde `main` actualizado (la que asigne la sesión). Un commit por
  punto (`feat(manga): …`), un PR por fase, título en inglés: **el título del
  PR es la nota de la release**.
- Cada fase es usable sola y deja el CI verde. Al final de cada fase,
  actualizar `CLAUDE.md` con su sección (como las de Discord o el diccionario).
- **No incluir mokuro.moe como preset ni nombrarlo en la UI.** Es un catálogo
  público de escaneos comerciales. El cliente es WebDAV genérico, compatible
  con mokuro-bunko; el usuario pone la URL. Probar contra él solo en
  desarrollo, en solo lectura y sin descargar volúmenes completos.
- Fuentes de terceros NSFW ocultas por defecto: se muestran solo con un ajuste
  explícito y nunca llegan a Discord (respetar `discord::decide`).
- Licencias: añadir a `THIRD_PARTY_LICENSES.md` los modelos (Apache-2.0),
  Suwayomi (MPL-2.0, se descarga sin modificar y se enlaza su código fuente) y
  ONNX Runtime (MIT).

---

## Fase M0 — Base

**M0.1 Servir imágenes al webview sin abrir el disco entero.** Registrar un
protocolo propio `manga://` con `register_asynchronous_uri_scheme_protocol` en
`lib.rs`. Las URLs son opacas: `manga://page/<volume_id>/<n>`,
`manga://cover/<id>`, `manga://thumb/<volume_id>/<n>`. Rust resuelve el id
contra la base de datos y sirve desde la carpeta, el CBZ, la caché de WebDAV o
Suwayomi. **Nunca rutas arbitrarias** del sistema. Cabeceras de caché y
`Content-Type` correctos.
- Miniaturas: generadas con el crate `image` (resize) y cacheadas en
  `app_cache_dir/manga/thumbs`. Hoy `image` solo está en Linux: pasarlo a todas
  las plataformas.

**M0.2 Archivos.** Mover `zip` a dependencias normales. Módulo
`src-tauri/src/manga/archive.rs` que lista las entradas de imagen de un
CBZ/ZIP:
- orden natural (`001, 2, 10`), ignorando `__MACOSX` y archivos ocultos;
- lectura de una entrada por streaming;
- límite de tamaño por entrada;
- protección contra zip bombs y rutas con `..`.

**M0.3 Esquema SQLite** (nuevas tablas en `SCHEMA`):
- `manga_series(id, source, source_ref, title, cover, media_id, is_adult, created_at)`
- `manga_volumes(id, series_id, title, source_ref, page_count, mokuro_path, ocr_state, added_at)`
- `manga_progress(volume_id PK, page, finished, updated_at)`
- `manga_page_text(volume_id, page, blocks_json, engine, PRIMARY KEY(volume_id,page))`:
  la caché del OCR, en formato de página `.mokuro`.
- `manga_page_read(volume_id, page, chars, read_at, PRIMARY KEY(volume_id,page))`:
  una página cuenta una sola vez.
- `manga_sources(id, kind, name, url, auth_json, created_at)`. El
  `auth_json` va en `kv` cifrado igual que la clave de API, o al menos nunca
  se expone a la UI. Comprobar cómo se guarda hoy la `api_key` y seguir el
  mismo patrón.

**M0.4 Tipos y parser `.mokuro`** en `manga/mokuro.rs`, con serde tolerante a
campos desconocidos. Tests con un `.mokuro` mínimo escrito a mano (no copiar
datos de mokuro.moe).

---

## Fase M1 — Biblioteca local y lector (la base estética)

**M1.1 Importar local.** Añadir una carpeta raíz con el diálogo existente.
Reglas de detección:
- carpeta de imágenes = un volumen;
- `.cbz` / `.zip` = un volumen;
- `<Vol>.mokuro` junto a `<Vol>.cbz` o a la carpeta `<Vol>/` = volumen con
  texto;
- la carpeta padre = la serie.

Reescaneo manual y al abrir. Evento `manga://library-changed`.

**M1.2 Pantalla Manga** (`src/screens/Manga.tsx` + `src/screens/manga/`), nueva
entrada en `navItems.ts` (y por tanto en la paleta, con atajo).
- Cabecera con buscador y pestañas de origen: Todo · Local · WebDAV ·
  Extensiones.
- Estante **Seguir leyendo**: portadas grandes con barra de progreso.
- Cuadrícula de series con portada 2:3, título en dos líneas, número de
  volúmenes y una insignia "OCR" si tiene texto. Hover con elevación
  `shadow-lg` y transición suave.
- Ficha de serie:
  - cabecera con la portada desenfocada de fondo;
  - botón "Leer" / "Continuar";
  - lista de volúmenes con progreso y estado del OCR;
  - vincular a NihongoTracker (`MediaSearchModal` con tipo `manga`).
- Vacío: `EmptyState` con las tres formas de añadir manga.
- Cargando: esqueletos de portada.

**M1.3 Lector** en su propia ventana `manga-reader`: nuevo caso en
`open_aux_window_inner`, label en `capabilities/default.json` y rama en
`App.tsx`.
- **Modos**: una página, doble página (con "desplazar una" para cuadrar los
  pares), tira vertical (webtoon).
- **Ajustes de lectura**:
  - dirección RTL por defecto;
  - ajuste a alto, ancho u original;
  - fondo negro, gris o papel.
- **Navegación**: clic en los bordes, flechas, rueda, `Space` / `Shift+Space`,
  `Home` / `End`, `F` (pantalla completa).
- **Interfaz que se esconde sola**:
  - barra superior con título, volumen y ajustes;
  - barra inferior con deslizador de páginas y tira de miniaturas al pasar el
    ratón.
- Precarga de ±2 páginas y transición de página corta (respetando
  `prefers-reduced-motion`).
- Ajustes por serie (modo y dirección) recordados en `manga_series` o en `kv`.
- Progreso guardado con debounce y al cerrar.

**M1.4 Capa de texto (estilo mokuro).**
- Cada bloque es una caja absoluta escalada desde
  `img_width/img_height`, con `writing-mode: vertical-rl` si `vertical`, y la
  fuente ajustada a la caja.
- Texto transparente que aparece al pasar el ratón. Ajuste: siempre visible,
  al pasar el ratón u oculto.
- Seleccionable, y `Ctrl+C` copia el bloque.
- Diccionario con el mismo gesto que el texthooker (Shift/Alt + hover),
  reutilizando `DictionaryPopup` y `textHitAtPoint`. Sacar la lógica común de
  `useDictionaryHover` a un hook compartido si hace falta, sin cambiar el
  texthooker.

---

## Fase M2 — Seguimiento, logs y minado

**M2.1 Conteo.**
- Al mostrar una página durante ≥ N s (ajustable, por defecto 3 s), se suman
  sus caracteres con `count_japanese` sobre sus bloques. Se registra en
  `manga_page_read`, que impide contarla dos veces.
- En doble página cuentan las dos.
- Sin texto (no hay OCR) cuentan la página y el tiempo, pero no los
  caracteres.

**M2.2 Cronómetro de sesión.** Lo lleva **Rust**, igual que las VN:
- solo cuenta con la ventana del lector visible y con foco;
- se pausa por inactividad con `afk_timeout_secs`;
- la UI recibe un evento `manga://session`.

**M2.3 Log.** Al cerrar el lector o tras M min de inactividad se arma el
payload:
- `{type:'manga', mediaId, description, date, pages, chars, time}`;
- `description` = "<serie> — <volumen>";
- `mediaId` = el vínculo de la serie.

Se manda por `sync::add_pending` si `vn_confirm_before_log` (o un ajuste propio
`manga_confirm_before_log`, por defecto activado), o por `enqueue_log`. Sin
`mediaId` va a Pendientes para vincularlo antes. Reutilizar `logRules.ts`.

**M2.4 Líneas y minado.**
- Cada bloque leído se inserta en `text_lines` (`content_id` = el `mediaId`, o
  `manga:<series_id>` si no hay vínculo), para que funcionen el minado con
  contexto y las estadísticas existentes.
- No llamar a `texthooker::ingest`: está atado al cronómetro del hooker.
- Minar desde un bloque: botón en su menú o `Ctrl+M`. Guarda el recorte del
  bloque, con un margen de viñeta, como PNG en `mined/` vía
  `set_mined_screenshot`; Anki lo recibe como imagen sin cambios.

**M2.5 Presencia de Discord**: "Leyendo <serie>", con la política de
`discord::decide` intacta (lista negra, `is_adult`, modo genérico). Una serie
de una fuente NSFW se marca `is_adult`.

**M2.6 Dashboard**: la tarjeta de sesión activa muestra la lectura de manga
(portada, páginas, caracteres, tiempo), siguiendo el patrón de `ActiveSessions`.

---

## Fase M3 — WebDAV

**M3.1 Cliente** (`manga/webdav.rs`, con reqwest):
- `PROPFIND Depth:1` con parseo XML usando el crate `quick-xml`;
- autenticación anónima, Basic o Bearer;
- timeouts y reintentos.

Detección de layout:
- con `mokuro-reader/` en la raíz, usarlo como base;
- cada carpeta es una serie;
- `<Vol>.cbz` + `<Vol>.mokuro` + `<Vol>.webp` forman un volumen;
- las capas `<Vol>.<x>.mokuro` se ofrecen como alternativas;
- también se aceptan carpetas de imágenes.

**M3.2 Formulario de origen**: URL, usuario/contraseña o token, y "Probar
conexión", que hace un PROPFIND y muestra cuántas series ve. Las credenciales
solo viven en Rust.

**M3.3 Lectura bajo demanda.**
- El `.mokuro` y la portada se bajan al abrir la serie.
- El CBZ (cientos de MB) se descarga **por rangos HTTP** si el servidor admite
  `Range`: el directorio central del ZIP y luego cada entrada al pedirla. Si no
  admite rangos, se descarga entero con barra de progreso.
- Caché LRU en `app_cache_dir/manga/webdav` con límite configurable (por
  defecto 2 GB) y botón "vaciar".
- "Descargar para leer sin conexión" por volumen.

**M3.4 (Opcional, al final)** Sincronizar el progreso con el formato de
Mokuro Reader. Antes de implementarlo, verificar el formato real de
`volume-data.json`, que no está comprobado.

---

## Fase M4 — OCR integrado (Rust + ONNX)

**M4.1 Motor** en `manga/ocr/`:
- `ort` con `load-dynamic`.
- Detector `comic-text-and-bubble-detector` (int8) → recorte por bloque →
  manga-ocr encoder/decoder (`onnx-community`, int8 por defecto, fp16
  opcional).
- Postproceso:
  - orientación vertical si la caja es más alta que ancha;
  - orden de lectura RTL (de arriba abajo, de derecha a izquierda);
  - salida en el formato de página `.mokuro`, guardada en `manga_page_text`.
- Corre en un hilo propio con cola y prioridad: la página visible primero,
  luego las siguientes.
- Nunca dentro del tick ni en un worker de tokio bloqueante: usar
  `spawn_blocking` o un hilo dedicado.
- Execution providers: DirectML en Windows, CoreML en macOS, CPU como
  reserva.

**M4.2 Descarga de modelos y runtime** solo con consentimiento:
- diálogo con el tamaño total (~170 MB int8 + runtime) y el espacio libre;
- descarga con progreso (`manga://ocr-progress`), reanudable;
- verificación SHA-256 fijada en el código, porque son binarios ejecutados;
- todo en `app_data_dir/ocr/`;
- botón "Eliminar modelos".

**M4.3 UX del OCR.**
- Insignia por volumen: sin texto / procesando x/y / listo.
- "Procesar volumen" en segundo plano, con notificación al terminar.
- OCR "al vuelo" de la página actual si no hay texto.
- Corregir un bloque a mano: editar el texto, y borrar o dibujar una caja.

**M4.4 Exportar** el resultado como `.mokuro` junto al volumen local, o
subirlo al WebDAV si el usuario tiene permiso de escritura. Así lo leen
Mokuro Reader y otros clientes.

---

## Fase M5 — owocr como motor alternativo

- Ajuste "Motor de OCR": Integrado · owocr.
- La app no instala owocr: el usuario lo arranca con `read_from=websocket`,
  `write_to=websocket` y `output_format=json`. La app enseña esos parámetros
  para copiar y prueba la conexión a `ws://127.0.0.1:7331`.
- Cliente en `manga/ocr/owocr.rs`:
  - envía la imagen de la página y espera el `"True"` y luego el JSON;
  - **una petición en vuelo cada vez** (no hay id de petición);
  - timeout, y si llega un resultado inesperado se descarta.
- Convertir sus cajas normalizadas (párrafo → líneas, `writing_direction`) al
  formato de bloque `.mokuro`.
- Avisar en la UI de que con el motor MangaOcr sin segmentar owocr no da
  cajas.

---

## Fase M6 — Extensiones (Suwayomi-Server + Keiyoushi)

**M6.1 Modo de servidor** (Ajustes → Manga → Extensiones):
- **Gestionado (recomendado)**: antes de descargar nada, un diálogo explica
  que se instalará Suwayomi-Server (servidor local en Java, MPL-2.0), cuánto
  ocupa (~183 MB del servidor + ~50 MB de Java 21) y dónde. Solo continúa si
  el usuario acepta.
- **URL propia**: URL, y usuario/contraseña si su servidor usa `basic_auth`.

**M6.2 Gestor del sidecar** (`manga/suwayomi.rs`):
- Descarga el `.jar` de la release fijada (versión y SHA-256 en el código;
  actualizar con un PR, no solo).
- Descarga un JRE 21 de Temurin (API de Adoptium) para el SO y la arquitectura.
- Arranca con:
  - `-Dsuwayomi.tachidesk.config.server.rootDir=<app_data>/suwayomi`;
  - `ip=127.0.0.1`, puerto libre aleatorio;
  - `systemTrayEnabled=false`, `initialOpenInBrowserEnabled=false`,
    `webUIEnabled=false`;
  - `authMode=basic_auth` con credenciales aleatorias guardadas en Rust.
- Espera a que `/api/graphql` responda.
- Se arranca solo al usar Extensiones y se detiene al cerrar la app
  (`RunEvent::Exit`) y tras X min sin uso.
- El proceso hijo no abre ventana en Windows (`CREATE_NO_WINDOW`, en
  `platform/`).
- Logs a archivo, y estado visible en la UI (descargando / arrancando / listo /
  error).

**M6.3 Cliente GraphQL** (reqwest + `serde_json`, sin dependencias nuevas
pesadas):
- La primera vez, `addExtensionStore(indexUrl:"https://github.com/keiyoushi/extensions/raw/repo/index.pb")`.
  Comprobar que Suwayomi acepta esa URL; si pide `repo.json`, usar
  `https://raw.githubusercontent.com/keiyoushi/extensions/repo/repo.json`.
- `fetchExtensions`, `updateExtension(install|update|uninstall)`,
  `fetchSourceManga(SEARCH|POPULAR|LATEST)`, `fetchChapters` y
  `fetchChapterPages`.
- Las imágenes se piden a la REST de binarios y se sirven al webview por
  `manga://`, para que las credenciales no lleguen al webview.

**M6.4 Tienda de extensiones**:
- Tarjetas con icono, nombre, idioma, versión e insignia de contenido.
- Filtros: idioma (por defecto `ja`), instaladas, con actualizaciones.
- Las extensiones NSFW y MIXED ocultas por defecto, tras un ajuste explícito
  "Mostrar fuentes para adultos".
- Botones instalar / actualizar / desinstalar con estado en vivo.
- Avisar de que las extensiones son código de terceros.

**M6.5 Explorar y leer**:
- Por fuente: populares, recientes y búsqueda, en una cuadrícula infinita con
  el mismo diseño que la biblioteca.
- La ficha de la serie trae capítulos de la fuente; "Añadir a la biblioteca"
  la guarda en `manga_series` con `source='suwayomi'`.
- El lector es el mismo de M1; el OCR (M4/M5) se aplica a las páginas de la
  fuente y se cachea por capítulo.
- Descarga de capítulos para leer sin conexión, usando la de Suwayomi
  (`chapter/{id}/download`).

---

## Fase M7 — Pulido

- Atajos visibles en una hoja de ayuda (`?`).
- Accesibilidad: el foco recorre los bloques con el teclado.
- Animaciones de portada a lector (View Transitions si WebView2/WebKit las
  soportan; si no, un fundido).
- Rendimiento:
  - decodificar imágenes con `decode()` antes de mostrarlas;
  - limitar los nodos de la tira vertical (ventana de páginas, como el
    texthooker de la Fase 2);
  - miniaturas en lugar de páginas completas en la tira.
- Capturas en tema claro y oscuro de biblioteca, ficha, lector y tienda,
  revisadas antes de cerrar la fase.

---

## Verificación

Tras cada fase:

```bash
cd src-tauri && CC=gcc-14 CXX=g++-14 cargo test && cd ..
npm run typecheck && npm test && npm run check:i18n && npm run build
npm run bindings   # commitear src/lib/bindings si cambia
```

Tests mínimos:

- Parser `.mokuro` con campos desconocidos.
- Orden natural de páginas y rechazo de `..` y zip bombs en CBZ.
- La URL `manga://` no sirve ids inexistentes ni rutas.
- Una página no cuenta dos veces y el payload del log cumple `logRules`.
- Parseo de un PROPFIND real guardado como fixture (escrito a mano con la
  misma forma).
- Conversión de cajas de owocr a bloques `.mokuro`.
- Postproceso del OCR (orden RTL, vertical) con cajas sintéticas.
- Parseo de respuestas GraphQL de Suwayomi (fixtures).
- Checksum incorrecto = descarga rechazada.

Pruebas manuales (Playwright con Tauri simulado en el contenedor; la app real
la prueba el autor en Windows):

- Una carpeta con un CBZ y su `.mokuro`: portada, lectura RTL, doble página,
  hover con diccionario, minado con recorte, log en Pendientes con páginas,
  caracteres y tiempo.
- WebDAV contra un mokuro-bunko de prueba: listar, abrir un volumen por rangos
  y ver la primera página sin bajar el CBZ entero.
- OCR integrado en una página sin `.mokuro`: aparecen bloques y cuentan
  caracteres.
- owocr corriendo en local: mismo resultado con su motor.
- Suwayomi gestionado:
  - el aviso de tamaño aparece antes de descargar;
  - arranca sin abrir ventana ni navegador;
  - la tienda Keiyoushi lista extensiones `ja`, instala una y lee un capítulo;
  - al cerrar la app el proceso Java termina.
