# NihongoTracker Desktop Companion

Compañero de escritorio para [NihongoTracker](https://github.com/HitogiDev/NihongoTracker).
Detecta lo que estás viendo o leyendo y lo registra solo — al estilo de
[Taiga](https://github.com/erengy/taiga), pero para inmersión en japonés.

- **Anime automático.** Reconoce mpv, mpv.net, MPC-HC/BE, VLC, PotPlayer, GOM,
  KMPlayer y SMPlayer, extrae título y episodio del nombre del archivo, lo busca
  en NihongoTracker y registra el episodio al pasar el umbral de visionado.
- **Novelas visuales.** Vincula un `.exe` con una obra: al abrirla arranca la
  sesión, se abre el texthooker y el tiempo se cuenta solo mientras la ventana
  está activa. Sin pausar ni reanudar nada a mano.
- **Texthooker integrado.** Cliente WebSocket para Textractor / Agent con el
  mismo conteo de caracteres que la web (solo kana y kanji). Funciona sin
  conexión: todo va a SQLite y se sincroniza al volver la red.
- **Todo offline-first.** Cada log pasa por una cola (`outbox`) con reintentos,
  así que cerrar sesión, perder la red o apagar el router no pierde nada.

## Stack

| Capa | Elección | Por qué |
|---|---|---|
| Runtime | Tauri v2 (Rust + WebView del sistema) | ~15 MB de instalador y decenas de MB de RAM, no cientos como Electron |
| UI | React 19 + Vite 7 + Tailwind v4 + daisyUI 5 | Mismo design system que el frontend de NihongoTracker |
| Datos locales | SQLite (`rusqlite`, bundled) | Cola offline, líneas del texthooker, registro de obras |
| Red | `reqwest` desde Rust | Evita el CORS del backend (su allowlist solo tiene el dominio web) |

## Cómo funciona por dentro

```
src-tauri/src/
  platform/      Ventanas y procesos por SO (Windows implementado; stub en el resto)
  players.rs     Reproductores conocidos y limpieza del título de ventana
  parser.rs      Parser de nombres de release estilo Anitomy (con tests)
  watcher.rs     Bucle de 1 s: sesiones de anime y de VN, y su cierre en log
  texthooker.rs  Cliente WebSocket (ws://localhost:6677 por defecto)
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
5. Al superar el umbral (70 % de la duración del episodio por defecto) el log se
   envía o se aparca en **Pendientes**, según los ajustes.

### Detección de novelas visuales

El proceso registrado aparece → arranca la sesión, se apunta el texthooker a esa
obra y se abre la pantalla. El tiempo solo corre si la ventana está en primer
plano (configurable) y no hay inactividad más allá del AFK. Al cerrar el proceso
se compone el log con minutos y caracteres del texthooker.

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
