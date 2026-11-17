# Plan: rendimiento y UI/UX de NihongoTracker Desktop Companion

## Contexto

La app funciona, pero gasta CPU de más en reposo y la interfaz no convence al
autor. Leyendo el código salen causas concretas: el bucle de detección hace cada
segundo trabajo que casi nunca cambia (procesos, SQLite, eventos), dos pantallas
se re-renderizan enteras cada segundo, el texthooker pinta hasta 5.000 líneas de
golpe, y la UI tiene pantallas sobrecargadas (Dashboard, Ajustes), sin feedback
uniforme y con textos sin traducir.

Objetivo: menos CPU en reposo y con sesión activa, texthooker fluido con
sesiones largas, y una UI más clara, **sin cambiar el comportamiento de
detección, conteo ni sincronización**.

Fuera de alcance (va en otro plan): el lector de manga. Decisión ya tomada para
entonces: fuentes = archivos locales, **WebDAV** y **Suwayomi-Server por URL**
(las extensiones de Tachiyomi son APK y corren dentro de Suwayomi; la app solo
habla con su API). Tampoco entran aquí estadísticas locales, subtítulos de mpv
ni metas diarias.

## Reglas para quien lo ejecute

- Leer `CLAUDE.md` antes de empezar; sus reglas mandan (i18n en `en` y `es`,
  nada de clases interpoladas, comando nuevo = `commands.rs` → `lib.rs` →
  `ipc.ts`, `npm run bindings` si cambia un struct exportado, código de SO solo
  en `platform/`).
- Rama: `claude/nihongo-tracker-improvements-ulvu35`. Un commit por punto
  numerado, con título en inglés tipo `perf(watcher): …` / `fix(ui): …` (los
  títulos acaban en las notas de la release). No abrir PR salvo que se pida.
- Orden: Fase 1 → 2 → 3. Cada fase deja la app verde (ver Verificación).
- Nada de esto se ha medido: son conclusiones de lectura. Donde el plan dice
  "comprobar", comprobar antes de cambiar.

---

## Fase 1 — Rendimiento en Rust (el tick de `watcher.rs`)

**1.1 Dejar de refrescar todos los procesos cada segundo** — `watcher.rs::observe`
(hoy `refresh_processes_specifics(ProcessesToUpdate::All, …)` en cada tick).
- Refresco completo solo cada ~5 s medidos con `Instant` (el intervalo de sondeo
  es configurable, no contar ticks) y en el primer tick.
- Entre medias, `ProcessesToUpdate::Some(&pids)` con: el pid en primer plano, el
  pid de la sesión VN/anime activa y los pids de `platform::visible_windows()`
  que `sys` aún no conoce. Así una ventana nueva se detecta en el mismo tick y
  el cierre del juego en curso también (`remove_dead = true`).
- Reconstruir el mapa `running` solo cuando `sys` cambió.
- Mantener el test `observation_includes_the_running_executable_path`; añadir
  uno que cubra "pid nuevo visible entre dos refrescos completos".
- `commands.rs:230` y `:271` (`list_processes`) siguen con refresco completo: son
  a demanda.

**1.2 Contar pendientes con `COUNT(*)`** — `watcher.rs:164` usa
`list_pending().len()`. Añadir `Db::pending_count()` al lado de
`Db::queued_count()` (`db.rs:642`) y usarlo.

**1.3 Caché de `vn_apps` en memoria** — `list_vn_apps()` se llama por tick en
`watcher.rs:483` (`update_vn`), `watcher.rs:186` (`update_native_game_hook`) y
`reading_overlay.rs:169` (`sync_session`), y por línea en
`Db::chinese_filter_for` (`texthooker.rs` ingest).
- Caché dentro de `Db` (`Mutex<Option<Arc<[VnApp]>>>`) con
  `Db::vn_apps_cached()`. Invalidar en **todo** lo que escribe en `vn_apps`:
  `upsert_vn_app`, `delete_vn_app`, `set_native_hook_code`,
  `set_vn_image_rating`, `restore_from` y cualquier otro `UPDATE vn_apps`
  (buscar en `db.rs`). Al vivir en `Db`, ningún llamador puede saltársela.
- En `tick`, pedirla una vez y pasarla a `update_vn`,
  `update_native_game_hook` y `sync_session`.
- `list_vn_apps()` se queda para los comandos de la UI.
- Test: editar una obra invalida la caché (basarse en
  `chinese_filter_is_per_work_and_falls_back_to_global`).

**1.4 No recalcular totales de texto cada segundo** — con una VN abierta,
`watcher.rs:611-612` ejecuta `text_totals` (COUNT/SUM sobre todas las líneas de
la obra) y `get_text_state` en cada tick.
- Caché write-through en `Db` por `content_id` para ambos: `insert_line` suma si
  la entrada existe; `delete_line`, `clear_lines`, restaurar línea y
  `restore_from` la invalidan; `save_text_state` actualiza la del estado.
- Test: insertar/borrar/limpiar deja `text_totals` igual que la consulta directa.

**1.5 Emitir `tracker://state` solo cuando cambia** — `watcher.rs:166-170`.
- Derivar `PartialEq` en `TrackerSnapshot` y sus tipos anidados (`state.rs`,
  más `parser`/`platform` si hace falta). No cambia los bindings.
- Función pura `should_emit(prev, next)`: iguales ignorando `idle_seconds`
  cuando no hay sesión VN (solo `VnCard` lo usa) → no emitir. Con test.
- Las ventanas nuevas ya piden el estado con `get_tracker_state` al arrancar.
- Comprobar que `discord::update` y `native_bridge::publish_status` ya
  deduplican por dentro; si no, llamarlos solo cuando hubo cambio.
- Comprobar si `platform::capabilities()` hace trabajo real por llamada (DBus en
  Linux); si es estático, cachearlo en `OnceLock`.

**1.6 SQLite** — `Db::open` (`db.rs:280`): añadir `synchronous = NORMAL` (seguro
con WAL, un `fsync` menos por línea ingerida). Usar `prepare_cached` en las
consultas calientes que queden (`queued_count`, `pending_count`, `insert_line`,
`latest_line`).

**1.7 Perfil de release** — `src-tauri/Cargo.toml` no tiene `[profile.release]`.
Añadir `lto = "thin"`, `codegen-units = 1`, `strip = true`. **No** poner
`panic = "abort"`: hoy un pánico en una tarea queda aislado (hilo de Discord,
`expect("db mutex poisoned")`) y eso lo convertiría en cierre de la app.

**1.8 Migraciones que no tragan errores** — `db.rs::migrate` hace `let _ =` con
cada sentencia de `MIGRATIONS`. Ignorar solo el error esperado ("duplicate
column name") y registrar con `log::warn!` cualquier otro. Sin cambiar el
esquema.

---

## Fase 2 — Rendimiento en el frontend

**2.1 Identidad estable del snapshot** — `src/store/useAppStore.ts`, listener de
`events.tracker`: guardar `replaceEqualDeep(get().tracker, payload)` (exportado
por `@tanstack/react-query`, ya es dependencia). Los sub-objetos que no cambian
(`game_hook`, `capabilities`, `anime` en pausa, `media`) conservan referencia y
los selectores dejan de disparar. Test en `useAppStore.test.ts`.

**2.2 Dashboard sin render por segundo** — `src/screens/Dashboard.tsx:54` se
suscribe a `s.tracker` entero.
- Selectores primitivos en el cuerpo (`platform_supported`, las dos capacidades
  del subtítulo, `queued_count`, `pending_count`), como ya hace `Sidebar.tsx`.
- Extraer `ActiveSessions` (único que lee `tracker.anime`, `tracker.vn`,
  `idle_seconds`). `FriendsImmersing`, `SocialActivityFeed`, progreso y logs
  recientes dejan de re-renderizarse.
- Igual en `SessionOverlay.tsx:15`.

**2.3 Trocear `src/screens/Texthooker.tsx`** (2.563 líneas, ~50 `useState`).
Refactor mecánico, **sin tocar DOM ni clases** (`th-root`, `th-topbar`,
`th-line-item`…: es un port de `HookerScreen.tsx` de la web). El export por
defecto sigue en `src/screens/Texthooker.tsx` (lo carga `App.tsx` con `lazy`).
- Carpeta `src/screens/texthooker/`: `TopBar`, `LineList` (+ `LineItem`, ya con
  `memo`), `SettingsPanel` (≈ líneas 1654–2260), `StatsModal` (≈ 1420–1650),
  `TimerEditModal`, `SessionPickerModal`, y hooks `useHookerLines`,
  `useManualTimer`, `useRoom`, `useDictionaryHover`.
- Cada modal lleva su propio estado y monta su contenido solo cuando está
  abierto: abrir ajustes o editar el cronómetro no re-renderiza la lista.
- El reloj se suscribe solo (`SessionClock`): el segundo que pasa no
  re-renderiza la pantalla. Sustituir `useAppStore((s) => s.tracker)` (línea
  206) por selectores de `vn` y `game_hook`.
- `sessionIntelligence` (`useMemo` sobre todas las líneas, línea 833):
  comprobar quién lo consume; si solo el modal de estadísticas, calcularlo ahí.
- Un commit por pieza extraída, para que el diff sea revisable.

**2.4 Lista de líneas** (`lines.map` directo, línea 2275, hasta 5.000 nodos).
- Paso A: `content-visibility: auto` + `contain-intrinsic-size` en
  `.th-line-item` (`src/index.css`); en vertical el tamaño es fijo
  (`COLUMN_HEIGHT`). Sin dependencias y sin cambiar el DOM.
- Paso B: pintar solo las últimas ~300 (`lines.slice(-k)`) con un centinela
  (`IntersectionObserver`) al principio que amplía `k`. El array completo sigue
  en memoria: estadísticas, deshacer, copiar todo y salas no cambian. Ajustar el
  scroll a mano al ampliar (WebKit no tiene scroll anchoring), en horizontal y
  en vertical (scroll RTL), respetando `followLatestRef`.
- **Bug a corregir de paso**: `Db::lines_for` (`db.rs:775`) hace
  `ORDER BY created_at LIMIT ?`, o sea, devuelve las **más antiguas**. Con más
  de 5.000 líneas, al recargar no salen las nuevas. Cambiar a las últimas N en
  orden ascendente (subconsulta `DESC LIMIT` + `ASC`), con test.

**2.5 Carga de rutas** — `App.tsx` usa `<Suspense fallback={null}>`: pantalla en
blanco mientras baja el chunk. Usar `PageLoader` (`components/ui/Spinner.tsx`).

---

## Fase 3 — UI y UX

**3.1 Textos sin traducir (bug)** — `Dashboard.tsx` ≈ línea 271: ` ep.`,
` min`, ` car.` escritos a mano; `components/ui/Modal.tsx`: `close` literal en
el backdrop. Pasarlos a i18n (claves en `en` y `es`) y repasar con grep el resto
de `src/` (incluido `lib/format.ts::logTypeLabel`) en busca de más literales.

**3.2 Toasts** — no hay feedback uniforme. Añadir `src/store/useToastStore.ts` y
`components/ui/Toaster.tsx` (clases `toast` + `alert` de daisyUI,
`aria-live="polite"`, cierre a los ~4 s, errores hasta que se cierren).
Montarlo en `App.tsx` para `main`, `settings` y `texthooker`. Migrar el feedback
suelto: `miningFeedback` del texthooker, sincronizar (Dashboard), guardar/borrar
en Biblioteca, exportar a Anki (Mining), copia de seguridad, importar
diccionario.

**3.3 Ajustes** — `src/screens/Settings.tsx` (628 líneas, 5 pestañas; "Tracking"
y "App" son muy largas y los diccionarios están escondidos bajo "Anki").
- Partir en `src/screens/settings/`: un archivo por sección.
- Navegación vertical a la izquierda (es una ventana propia) con: Cuenta ·
  Anime · Novelas visuales y juegos · Overlay de lectura · Diccionarios · Anki ·
  Discord · Apariencia y app · Copias y actualizaciones · Avanzado.
- Recordar la última sección (`localStorage`); flechas arriba/abajo en la nav.
- Buscador (prioridad baja, último): índice estático `{sección, claves i18n}`;
  al elegir un resultado, ir a la sección y resaltar la fila.

**3.4 Dashboard** — hoy: avisos → sesiones → niveles → amigos → feed social →
logs → botón de pendientes, todo en una columna.
- Nuevo orden: avisos → sesión activa → **Hoy** → logs recientes → progreso →
  comunidad (amigos + feed) plegable, con el estado recordado.
- El aviso de pendientes sube junto a los avisos (hoy está al final).
- **Hoy**: minutos, caracteres, episodios y racha del día. Comando nuevo
  `today_summary` en Rust que suma los payloads del outbox (en cola + enviados)
  del día actual; el día lo decide `streak.rs` con la zona de la cuenta, nunca
  el frontend. Etiquetarlo "en este equipo": los logs hechos desde la web no
  están en el outbox. Struct con `ts_rs`, bindings, wrapper en `ipc.ts`; se
  refresca con `outbox://changed` (ya se escucha en el Dashboard).

**3.5 Onboarding guiado** — `src/screens/Onboarding.tsx` es un solo paso
(servidor + clave). Tras conectar, pasos omitibles antes de ir a `/`:
1. Qué seguir: toggles `detect_players`, `detect_visual_novels`,
   `detect_games`; si hay mpv en marcha, ofrecer `MpvSetupPrompt`
   (`components/MpvSetup.tsx`).
2. Primera obra: abrir el alta de la Biblioteca (extraer el modal de alta de
   `Library.tsx` para reutilizarlo).
3. Diccionario: importar (extraer `importDictionary` de `Settings.tsx` a un
   hook `useDictionaryImport` que usen los dos sitios).
Estado local; no hace falta ajuste nuevo.

**3.6 Paleta de comandos (Ctrl/⌘+K)** — `components/CommandPalette.tsx` sobre
`Modal`, abierta desde el `keydown` de `Sidebar.tsx` (donde viven los atajos).
Entradas: secciones, registrar log, abrir texthooker, sincronizar, ajustes, y
las obras de la biblioteca (`ipc.listVnApps` → `ipc.launchProgram` /
`ipc.openWorkTexthooker`). Comprobar que `lib/desktop.ts` no anula Ctrl+K en
producción.

**3.7 Estados de carga, vacío y error** — varias consultas van con
`retry: false` y sin UI de error (p. ej. logs recientes: si falla, desaparece).
Repasar Queue, Library, Mining, Ranking, Social y Dashboard con la misma pauta:
esqueleto al cargar, `EmptyState` (`components/ui/Empty.tsx`) vacío, y error con
botón de reintentar.

**3.8 Detalles** — recordar la vista cuadrícula/lista de `Library.tsx` (hoy
`useState('grid')`); anillos `focus-visible` coherentes; respetar
`prefers-reduced-motion` en las animaciones nuevas.

---

## Verificación

Tras cada fase:

```bash
cd src-tauri && cargo check && cd ..
npm run test:rust
npm test
npm run typecheck
npm run check:i18n
npm run bindings   # y commitear src/lib/bindings si cambió (3.4)
npm run build
```

Si el contenedor no puede compilar la app (faltan GTK/WebKit), dejarlo dicho y
apoyarse en el CI, que compila en los tres sistemas.

Tests nuevos mínimos: `should_emit`, `pending_count`, invalidación de la caché
de `vn_apps`, caché de totales de texto, `lines_for` devuelve las últimas N,
identidad estable en `useAppStore`, `useToastStore`, `today_summary` con un día
que cruza medianoche en una zona al oeste de UTC.

Comprobación manual en Windows (la hace el autor, con `npm run tauri dev`):
- CPU en reposo del proceso en el Administrador de tareas, antes y después.
- Abrir y cerrar una VN: la sesión arranca en ≤1 s y termina al cerrar el juego.
- Reproducir un episodio en mpv: progreso y log igual que antes.
- Texthooker con una obra de miles de líneas: scroll fluido, la última línea
  sigue visible al llegar una nueva, modo vertical incluido, "cargar
  anteriores" no salta.
- Editar una obra en Biblioteca con el juego abierto: el cambio (filtro de
  chino, hook) se aplica sin reiniciar.
- Cambiar de idioma: ningún texto queda en el otro idioma.
