# NihongoTracker Desktop Companion

Compañero de escritorio para [NihongoTracker](https://github.com/HitogiDev/NihongoTracker).
Detecta lo que estás viendo o leyendo y lo registra solo — al estilo de
[Taiga](https://github.com/erengy/taiga), pero para inmersión en japonés.

- **Anime automático.** Reconoce mpv, mpv.net, MPC-HC/BE, VLC, PotPlayer, GOM,
  KMPlayer y SMPlayer, extrae título y episodio del nombre del archivo, lo busca
  en NihongoTracker y registra el episodio al pasar el umbral de visionado. Con
  mpv y MPC usa la **posición real** del vídeo, no el tiempo con la ventana
  abierta, así que una pausa no cuenta.
- **Novelas visuales.** Vincula un `.exe` con una obra: al abrirla arranca la
  sesión, se abre el texthooker y el tiempo se cuenta solo mientras la ventana
  está activa. Sin pausar ni reanudar nada a mano.
- **El texthooker de NihongoTracker, dentro.** Port fiel de `HookerScreen`: la
  misma barra flotante con sus métricas, el mismo cronómetro anclado a reloj de
  pared con pausa por inactividad, las mismas fuentes y temas, CSS personalizado
  con las mismas clases (`.th-root`, `.th-topbar`, `.th-line-text`…), texto
  vertical, borrar línea y deshacer. Cliente WebSocket para Textractor / Agent
  con idéntico conteo de caracteres (solo kana y kanji), más un lector del
  portapapeles que sustituye a la extensión de navegador. Funciona sin conexión:
  todo va a SQLite y se sincroniza al volver la red.
- **Todo offline-first.** Cada log pasa por una cola (`outbox`) con reintentos,
  así que cerrar sesión, perder la red o apagar el router no pierde nada.
- **Aviso antes de perder la racha.** Si queda poco para que acabe tu día y
  todavía no has registrado nada, salta una notificación —una al día, y sólo
  después de volver a preguntarle al servidor— con un botón que abre el
  formulario de registro. Ver abajo.
- **Diccionarios emergentes dentro de la app.** Yomitan y JitenReader son
  extensiones de Chrome, y el webview de Windows puede cargarlas, así que
  funcionan sobre el propio lector. Ver abajo.
- **Discord Rich Presence, si quieres.** Apagado por defecto. Enseña qué estás
  leyendo o viendo, con portada y tiempo, y nunca el título de una obra +18.
  Ver abajo.
- **En inglés y español**, con selector en Ajustes y en la primera pantalla; el
  idioma por defecto es el del sistema.
- **Confirmar o no, tú eliges.** Por defecto cada detección espera tu visto bueno
  en *Pendientes*, con el XP estimado por el propio servidor; también puedes
  registrar el episodio en curso a mano sin esperar al umbral.

## Stack

| Capa | Elección | Por qué |
|---|---|---|
| Runtime | Tauri v2 (Rust + WebView del sistema) | ~15 MB de instalador y decenas de MB de RAM, no cientos como Electron |
| UI | React 19 + Vite 7 + Tailwind v4 + daisyUI 5 | Mismo design system y los mismos 19 temas que el frontend de NihongoTracker |
| Datos locales | SQLite (`rusqlite`, bundled) | Cola offline, líneas del texthooker, registro de obras |
| Red | `reqwest` desde Rust | Evita el CORS del backend (su allowlist solo tiene el dominio web) |

## Cómo funciona por dentro

```
src-tauri/src/
  platform/      Ventanas y procesos por SO (Windows implementado; stub en el resto)
  players.rs     Reproductores conocidos y limpieza del título de ventana
  parser.rs      Parser de nombres de release estilo Anitomy (con tests)
  watcher.rs     Bucle de 1 s: sesiones de anime y de VN, y su cierre en log
  texthooker.rs  Cliente WebSocket (ws://localhost:6677) + lector de portapapeles
  api.rs         Cliente REST con cabecera x-api-key
  sync.rs        Outbox con reintentos y sincronización de líneas
  db.rs          Esquema SQLite y consultas
  commands.rs    Superficie IPC que consume el frontend
src/
  screens/       Actividad, Pendientes, Biblioteca, Texthooker, Ajustes
  components/ui/ Button, Modal, Field, Page… copiados del design system web
```

### Autenticación

El backend acepta `x-api-key` **antes** que la cookie JWT
(`Backend/src/middlewares/authMiddleware.ts`), y esa cookie es `httpOnly` +
`sameSite: strict`, así que es inusable desde una app nativa. El companion pide
una API key (Ajustes → API keys en la web, formato `ntk_…`) y la guarda en el
SQLite local.

### Detección de anime

1. Se listan las ventanas visibles y se busca un proceso de reproductor conocido.
2. Se limpia la marca del reproductor del título (`… - mpv`) para quedarse con el
   nombre del archivo.
3. `parser.rs` saca título, episodio, temporada y grupo.
4. El título se resuelve contra `GET /api/media/search?type=anime`, con caché en
   memoria y una tabla de vinculaciones manuales que tiene prioridad.
5. Al superar el umbral (70 % del episodio por defecto) el log se envía o se
   aparca en **Pendientes**, según los ajustes.

#### Saber por dónde va el capítulo

El tiempo con la ventana abierta es el plan B: un vídeo en pausa también suma.
Cuando el reproductor puede decir su posición, se le pregunta:

| Reproductor | Cómo | Qué hay que activar |
|---|---|---|
| mpv, mpv.net | IPC JSON por named pipe / socket | `input-ipc-server=/tmp/mpv-socket` en `mpv.conf`; en Windows mpv lo convierte en `\\.\pipe\tmp\mpv-socket` y el companion hace la misma traducción |
| MPC-HC, MPC-BE | `variables.html` de su interfaz web | Opciones → Reproductor → Interfaz web → *Escuchar en el puerto* 13579 |
| Los demás | tiempo transcurrido | — |

Con posición real el progreso es `posición / duración`, y una pausa deja de
contar. Las rutas y el puerto se cambian en Ajustes → Anime.

### Detección de novelas visuales

El proceso registrado aparece → arranca la sesión, se apunta el texthooker a esa
obra y se abre la pantalla. El tiempo solo corre si la ventana está en primer
plano (configurable) y no hay inactividad más allá del AFK. Al cerrar el proceso
se compone el log con minutos y caracteres del texthooker.

### Racha en riesgo

El aviso mide el día **en la zona horaria de tu cuenta de NihongoTracker**, no en
la de este ordenador: si viajas o tienes mal la zona del sistema, el recordatorio
sigue cayendo el día correcto. Salta cuando faltan menos de X horas para tu
medianoche (3 por defecto, configurable) y, si quieres, también a una hora fija.

Para no molestar:

- Una notificación por día como mucho, aunque cierres y abras la app.
- Justo antes de avisar se vuelve a preguntar al servidor, así que un log hecho
  desde la web o el móvil hace un minuto cancela el aviso sin que te enteres.
- Si la app estaba cerrada y la abres pasada la hora, no te grita avisos
  atrasados.
- El clic abre el formulario de registro (en Windows; en macOS y Linux el
  webview no puede recibir el clic del sistema, así que ahí la notificación
  sólo informa y el formulario se abre desde la bandeja o desde *Actividad*).

Se apaga en Ajustes → Aplicación, junto con la hora fija y las horas de aviso.
Ojo: un log con fecha desconocida no cuenta para la racha en el servidor, así
que el aviso hace caso a `lastStreakDate`, no a lo que hayas enviado tú.

### Discord Rich Presence

Opcional y **apagado por defecto**: mientras no lo enciendas no se publica nada
ni se abre ninguna conexión con Discord.

Para encenderlo hace falta una aplicación propia en el
[Discord Developer Portal](https://discord.com/developers/applications): copia su
*Application ID* en Ajustes → Discord Rich Presence y sube tus iconos en
*Rich Presence → Art Assets* (el nombre del asset de reserva se configura ahí
mismo). Discord no tiene que estar abierto para arrancar la app: si no lo está,
se reintenta solo.

Lo que se publica:

| | |
|---|---|
| Línea 1 | "Leyendo 蒼の彼方のフォーリズム" / "Viendo Frieren" |
| Línea 2 | "12,4k caracteres" / "Episodio 3" |
| Tiempo | Desde que empezó la sesión, contado por Discord |
| Imagen | La portada de la obra, o tu icono de reserva |
| Botón | "Ver perfil", a tu perfil público (se puede quitar) |

Y lo que **no** se publica nunca:

- El título ni la portada de una obra marcada +18 en NihongoTracker. Cae al modo
  genérico: "Inmersión en japonés".
- El nombre del archivo cuando no se pudo identificar la obra.
- Nada en absoluto de lo que hayas metido en la lista negra, por obra o por
  ejecutable — ni siquiera una actividad genérica.
- Nada mientras la sesión esté en pausa, ni después de cerrar la app o de apagar
  el toggle: la presencia se borra, no se queda colgada con lo de anoche.

También hay un modo de **solo actividad** que oculta cualquier título, sea de lo
que sea. En Ajustes tienes un preview en vivo de la presencia exacta que se
publicaría en este momento —lo dibuja la misma función que decide lo que se
manda—, así que puedes comprobarlo antes de encender nada.

### Yomitan y JitenReader

Los dos son extensiones de navegador, no aplicaciones. WebView2 sabe cargar
extensiones de Chrome sin empaquetar, y Tauri lo expone, así que la ventana
principal se construye en Rust (`build_main_window`) con
`browser_extensions_enabled` y una carpeta de extensiones propia.

1. Ajustes → Diccionarios emergentes → **Abrir carpeta**.
2. Descomprime ahí la build de Chrome de la extensión, una carpeta por
   extensión (para Yomitan sirve `yomitan-chrome.zip` o `yomitan-edge.zip` de
   sus releases; JitenReader se puede cargar igual desde su repo).
3. Reinicia la app. La extensión aparece listada con su id y su versión.
4. El botón **Ajustes** de cada extensión abre su propia página de
   configuración en una ventana aparte — es lo que necesitas para importar
   diccionarios en Yomitan o meter la API key de Jiten.

Como el webview no tiene barra de herramientas, la app calcula el id que
Chromium le daría a la extensión (SHA-256 de su ruta) para poder enlazar a sus
páginas internas. Verificado contra un perfil real de WebView2.

**Limitación:** esto es solo Windows. WebView2 admite extensiones; WKWebView
(macOS) y WebKitGTK (Linux) no admiten extensiones de Chrome, así que ahí el
ajuste no hace nada. La alternativa en esas plataformas es leer con el
texthooker de la web en tu navegador, con la extensión instalada allí.

## Desarrollo

Requisitos: Node 20+, Rust estable, y en Windows el WebView2 Runtime y las
Build Tools de MSVC.

```bash
npm install
npm run tauri dev      # app en modo desarrollo
npm run build          # solo el bundle del frontend
npm run tauri build    # instalador (NSIS en Windows)
cd src-tauri && cargo test   # tests del parser y de los reproductores
```

## Estado por plataforma

| | Detección | Texthooker | Notas |
|---|---|---|---|
| Windows | ✅ | ✅ | `EnumWindows` + `GetLastInputInfo` |
| Linux | ⏳ | ✅ | Falta la capa X11/Wayland en `platform/` |
| macOS | ⏳ | ✅ | Falta la capa de Accessibility en `platform/` |

La interfaz y toda la lógica ya son multiplataforma: solo `platform/` necesita
una implementación nativa por sistema.
