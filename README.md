# NihongoTracker Desktop Companion

Desktop companion for [NihongoTracker](https://github.com/HitogiDev/NihongoTracker).
It detects what you are watching, reading, or playing and logs it for you, like
[Taiga](https://github.com/erengy/taiga) but for Japanese immersion. It runs on
Windows, Linux, and macOS (see [Platform status](#platform-status)).

## Features

- **Automatic anime tracking.** Recognizes mpv, mpv.net, MPC-HC/BE, VLC,
  PotPlayer, GOM, KMPlayer, and SMPlayer (and, on Linux, any player that exposes
  MPRIS). It extracts the title and episode from the filename, searches
  NihongoTracker, and logs the episode once the viewing threshold is reached.
  With mpv and MPC it uses the video's real position instead of the time the
  window has been open, so a paused video does not count. You can also log the
  current episode right away without waiting for the threshold.
- **Visual novels.** Link an `.exe` to a title. Opening it starts a session,
  opens the texthooker, and counts time only while the window is focused and you
  are not AFK. Nothing to pause or resume by hand.
- **Games.** Link a game's executable in the Library. Time counts only while the
  game is focused and you are not AFK, and the session goes to *Pending* for
  review when the game closes.
- **NihongoTracker's texthooker, built in.** A close port of the web
  `HookerScreen`: same floating bar and metrics, same wall-clock timer with an
  inactivity pause, same fonts and themes, custom CSS with the same classes
  (`.th-root`, `.th-topbar`, `.th-line-text`, …), vertical text, line deletion,
  and undo. Text comes from Textractor / Agent over WebSocket, the clipboard, or
  the built-in LunaHook integration on Windows. Characters are counted exactly
  like the website (kana and kanji only).
- **Shared rooms.** Host or join a texthooker room. Rooms use the same protocol
  as the website, so desktop and browser readers can share one.
- **Text filters.** Remove duplicated phrases, excluded words, regex matches, and
  Chinese translations that some releases hook together with the Japanese text.
- **Offline dictionary.** Import Yomitan dictionary zips and look up words from
  the texthooker or the reading overlay with no browser or network connection.
- **Sentence mining.** Save any line, or press Ctrl+Shift+M (⌘⇧M on macOS) from
  any window, with an optional screenshot of the game. Send sentences and words
  to Anki through AnkiConnect.
- **Reading overlay.** A transparent dialogue overlay on top of the VN, with
  hover lookups and one-click Anki cards.
- **Floating session panel.** An optional always-on-top window with the active
  work, timer, latest line, and quick actions.
- **Community.** Friend activity, friends immersing right now, and the
  leaderboard, without opening the website. Optionally share what you are
  immersing in with mutual followers.
- **Offline first.** Every log goes through a local outbox with retries, so
  closing the app or losing the network never loses data.
- **Streak reminder.** One notification per day when your streak is at risk,
  re-checked against the server before it fires.
- **Optional Discord Rich Presence.** Off by default, and it never shows the
  title of adult media.
- **Self-update** from GitHub Releases. Nothing installs until you confirm it.
- **English and Spanish.** The default follows the operating system.

### Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Ctrl/⌘ + 1…6 | Activity, Pending, Library, Sentences, Friend activity, Ranking |
| Ctrl/⌘ + , | Settings |
| Ctrl/⌘ + L | Manual log form |
| Ctrl/⌘ + T | Texthooker |
| Ctrl+Shift+M (⌘⇧M) | Mine the latest line from any window |
| Ctrl+Shift+O / Ctrl+Shift+I | Show or hide the reading overlay / toggle click-through (configurable) |

## Stack

| Layer | Choice | Reason |
|---|---|---|
| Runtime | Tauri v2 (Rust + system WebView) | A ~15 MB installer and tens of MB of RAM instead of hundreds like Electron |
| UI | React 19 + Vite 7 + Tailwind v4 + daisyUI 5 | Shares the design system and themes of the NihongoTracker frontend |
| Local data | SQLite (`rusqlite`, bundled) | Offline queue, texthooker lines, library, and mined sentences |
| Network | `reqwest` from Rust | Avoids the backend CORS allowlist, which only includes the web domain |
| Dictionary | [Hoshidicts](https://github.com/Manhhao/hoshidicts) (C++, git submodule) | Yomitan-compatible import, lookup, and deinflection |

## How it works

All detection, networking, and persistence live in Rust. The frontend is a view:
it listens to events and calls commands, and never talks to the NihongoTracker
API directly.

```
src-tauri/src/
  platform/        Windows, focus, idle time and players per OS (Win32, X11, D-Bus/MPRIS, CoreGraphics)
  players.rs       Known players and window-title cleanup
  parser.rs        Anitomy-style release-name parser (with tests)
  playback.rs      Real position from mpv IPC and the MPC web interface
  watcher.rs       Detection loop for anime, VN and game sessions; turns them into logs
  texthooker.rs    WebSocket client (ws://localhost:6677) and clipboard reader
  native_hook.rs   LunaHook integration (Windows)
  text_filters.rs  Duplicate, word, regex and Chinese filters
  dictionary.rs    Offline dictionary import and lookup
  mining.rs        Sentence mining, screenshots and Anki export
  reading_overlay.rs  Transparent VN overlay window
  streak.rs        Streak reminder
  discord.rs       Optional Rich Presence
  updater.rs       Self-update from GitHub Releases
  api.rs           REST client (x-api-key)
  sync.rs          Outbox with retries and line sync
  db.rs            SQLite schema and queries
  commands.rs      IPC surface used by the frontend
crates/nihongo-dict/       Rust FFI over the Hoshidicts engine
packages/yomitan-content/  Structured-content renderer shared with the web frontend
src/
  screens/         Activity, Pending, Library, Sentences, Friends, Ranking, Texthooker, Settings, Onboarding
  components/ui/   Design system copied from the web frontend
  i18n/            English and Spanish translations
```

### Authentication

The backend checks the `x-api-key` header before the JWT cookie
(`Backend/src/middlewares/authMiddleware.ts`). The cookie is `httpOnly` and
`sameSite: strict`, so a native app cannot use it. The companion asks for an API
key (`ntk_...`, created on the website under Settings → API keys) and stores it
in the local SQLite database.

### Anime detection

1. The app lists visible windows (or MPRIS players) and looks for a known media
   player.
2. It strips the player's marker from the title, such as `... - mpv`, to get the
   filename.
3. `parser.rs` extracts the title, episode, season, and release group.
4. The title is resolved with `GET /api/media/search?type=anime`, using an
   in-memory cache and a manual-link table that takes priority.
5. When the threshold is reached, the log is sent or placed in **Pending**,
   depending on your settings. XP is always estimated by the server.

#### Episode progress

Time with the window open is only the fallback. When the player exposes its
position, the companion uses it:

| Player | Method | Required setting |
|---|---|---|
| mpv, mpv.net | JSON IPC over a named pipe or socket | `input-ipc-server=/tmp/mpv-socket` in `mpv.conf`. On Windows mpv turns this into `\\.\pipe\tmp\mpv-socket`, and the companion does the same |
| MPC-HC, MPC-BE | `variables.html` from the web interface | Options → Player → Web interface → *Listen on port* 13579 |
| MPRIS players (Linux) | D-Bus | For mpv, the [mpv-mpris](https://github.com/hoyon/mpv-mpris) plugin |
| Other players | Elapsed time | None |

With real position data, progress is `position / duration` and pausing adds no
time. Change the socket path and port under Settings → Tracking → Anime.

### Visual novels

When a registered process appears, the companion starts the session, assigns the
texthooker to that title, and opens it. The timer runs only while the window is
in the foreground (if configured) and while you are not idle past the AFK limit.
When the process closes, it creates a log with the session minutes and the
texthooker character count.

### Games

Choose *Game* when adding an executable in the Library and link it to a game on
NihongoTracker. Games use the same process watcher as visual novels, but always
require focus and never open the texthooker on their own; open it from the
active session or the sidebar when you want it. On exit the session goes to
*Pending*, where you can review the minutes before sending it.

### LunaHook (Windows)

Enable **Capture game text with LunaHook** on a game or VN in the Library. The
companion picks the x86 or x64 helper to match the game and sends the captured
Japanese text to its texthooker, with no Textractor needed.

In the texthooker, click **LunaHook** to open the hook selector (it also opens
on its own the first time a game has no saved choice). Advance the dialogue to
see live samples, pick the source, and click **Save hook**. The choice is saved
per executable and reused on the next launch. Building a release with both
helpers requires the C++ build tools for x86 and x64
(`npm run build:native-game-hook`).

### Text filters

Under Settings → Tracking → Text filters (also reachable from the texthooker and
the overlay settings):

- **Remove duplicate sentences**: `こんにちはこんにちは` → `こんにちは`, while
  keeping repeated characters such as `ああ……`.
- **Filter out Chinese lines**: for releases that hook a Chinese translation next
  to the Japanese. Lines with kana are never treated as Chinese; names and other
  short kanji-only lines are kept. A translation glued to a Japanese line
  (`「え…」“啊……”`) is cut off.
- **Excluded words** and **regex rules**, with a live tester.

Filtered text is not shown and does not count toward the characters in your
logs. Existing lines stay as they are.

### Offline dictionaries

Go to **Settings → Anki → Offline dictionaries → Import dictionary** and pick a
dictionary `.zip` exported for Yomitan. It is indexed and stored in the app's
local data, so lookups need no connection or account.

In the texthooker, hover over a word while holding **Shift** or **Alt**. The
popup shows entries from every imported dictionary and lets you follow internal
references. Structured Yomitan content is rendered safely: imported dictionaries
cannot inject HTML, global styles, or external links.

The engine is [Hoshidicts](https://github.com/Manhhao/hoshidicts) `main`,
which provides the Yomitan-compatible importer, lookup strategy, frequency and
pitch metadata, and Yomitan's Japanese deinflection rules.

### Sentence mining and Anki

Save a sentence with the bookmark beside a texthooker line, the reader toolbar,
the floating panel, or Ctrl+Shift+M. A line is saved only once, so repeating the
shortcut does not create duplicates. The shortcut also captures the focused game
window when the system allows it (Windows, Linux on X11, macOS with Screen
Recording). Everything stays local and works without an account.

From the *Sentences* screen, or the **+** button in the dictionary popup, cards
go to Anki Desktop through AnkiConnect (`127.0.0.1:8765`). Choose the deck, note
type, and what goes into each field (word, reading, meaning, sentence, media
title) under Settings → Anki. Anki and the add-on must be running. Audio clips
are not supported.

### Reading overlay

Edit a VN in the Library and choose **Reader → Overlay**. This enables LunaHook
and replaces the automatic reader window with a transparent dialogue overlay on
top of the game.

- Hover over a word to open the dictionary. The popup lives in its own window and
  keeps the sentence you looked up while new dialogue arrives.
- The toolbar appears on hover: pick a LunaHook source, move the overlay, open
  its settings, let clicks pass through, or hide it. Drag the bottom-right grip
  to resize it. Position and size survive restarts.
- **Overlay settings** (from the toolbar or Settings → Tracking) control font
  weight, alignment, background opacity, hover and idle visibility, lookup delay,
  window priority, and global shortcuts, and show shortcut conflicts.
- The overlay follows the VN process and hides when another app is in front. It
  does not change the timer, capture source, or sync.

The overlay uses captured text, not OCR, and targets windowed or borderless
games. Exclusive fullscreen is not supported.

### Community

The *Friend activity* screen shows what people you follow, your clubs, or the
whole community have been doing, plus friends who are immersing right now. The
*Ranking* screen is the NihongoTracker leaderboard, filterable by medium,
period, and audience (everyone, following, mutuals). Both need a connection.

Turn on **Share what I am immersing in** under Settings → App to let mutual
followers see your current tracked work. Sharing stops when tracking pauses or
the app closes.

### Streak reminder

The reminder uses the time zone of your NihongoTracker account, not your
computer's, so it follows the right day even if you travel or the system clock
is wrong. It fires when fewer than N hours remain before midnight (three by
default) or at a fixed time.

- At most one notification per day, even if you restart the app.
- It re-checks the server right before notifying: a log from the web or the
  phone a minute earlier cancels it.
- After a restart past the reminder time, old reminders are not shown.
- On Windows and Linux the notification has a **Log now** button. On macOS it is
  informational; open the form from the tray or *Activity*.

Logs with an unknown date do not count toward the streak on the server, so the
reminder checks `lastStreakDate`, not the logs you sent. Configure it under
Settings → App.

### Discord Rich Presence

Optional and **off by default**: until you enable it, nothing is published and no
Discord connection is opened.

Create an application in the
[Discord Developer Portal](https://discord.com/developers/applications), copy its
*Application ID* into Settings → Tracking → Discord Rich Presence, and upload
your icons under *Rich Presence → Art Assets* (set the fallback asset name there
too). Discord does not need to be running when the app starts; the companion
reconnects on its own.

| | |
|---|---|
| Line 1 | `Reading 蒼の彼方のフォーリズム` or `Watching Frieren` |
| Line 2 | `12.4k characters` or `Episode 3` |
| Time | From the start of the session |
| Image | The title's cover or your fallback icon |
| Button | `View profile`, linking to your public profile (optional) |

Never published:

- The title or cover of media marked as adult on NihongoTracker. It falls back
  to a generic `Japanese immersion` status.
- The filename when the title could not be identified.
- Anything on your blacklist, by title or by executable. Not even a generic
  status is shown.
- Anything after the app closes or you turn the toggle off. A paused video
  clears the presence; a paused VN shows an "idling" status unless you disable
  it.

An **activity-only** mode hides every title. Settings shows a live preview built
by the same function that publishes, so what you see there is exactly what
others will see.

## Development

Requirements: Node 20+, stable Rust, CMake, and Ninja. On Windows, the WebView2
Runtime and the MSVC Build Tools. On Linux, the usual Tauri libraries
(`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`,
`libxdo-dev`).

Hoshidicts is a git submodule, so initialize it after cloning:

```bash
git submodule update --init --recursive
```

```bash
npm install
npm run tauri dev      # full app in development (Vite on :1420 + Rust binary)
npm run dev            # frontend only, without Tauri commands
npm run typecheck      # tsc --noEmit
npm run build          # tsc + vite build
npm run test:rust      # cargo test (release parser, players, filters, streaks)
npm run tauri build    # installer (NSIS on Windows)
```

`.github/workflows/ci.yml` builds and tests the app on Windows, Linux, and macOS.
[CLAUDE.md](CLAUDE.md) documents the architecture and project rules in more
detail (in Spanish).

### Releases and updates

Installed copies check GitHub for a new release 30 seconds after start and every
six hours, notify once per version, and install only when you confirm it
(Settings → App, or the notice in the sidebar). On Linux only the AppImage
replaces itself; `.deb` and `.rpm` installs link to the release instead.

To publish a release, bump `version` in `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml`, and `package.json`, commit, and push a matching tag:

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

`.github/workflows/release.yml` builds and signs the installers for the three
systems and publishes them with the `latest.json` manifest the app reads. It
needs the repository secret `TAURI_SIGNING_PRIVATE_KEY`, matching
`plugins.updater.pubkey`. The repository must be public, since the app downloads
releases without authenticating.

The Hoshidicts submodule tracks its upstream `main` branch. Updating it is a
deliberate dependency update and requires rebuilding the native crate.

## Platform status

| | Windows | Linux (X11) | Linux (Wayland) | macOS |
|---|---|---|---|---|
| Players by window title | ✅ | ✅ | XWayland only | With Screen Recording |
| Players by media controls (MPRIS) | — | ✅ | ✅ | — |
| Real playback position | mpv, MPC | mpv, MPRIS | MPRIS | mpv |
| Novels and games | ✅ | ✅ (incl. Wine/Proton) | ✅ | ✅ |
| Focus-only counting | ✅ | ✅ | Assumed focused | ✅ |
| AFK timeout | ✅ | ✅ | GNOME / KDE | ✅ |
| Game screenshot when mining | ✅ | ✅ | — | With Screen Recording |
| "Log now" on the streak reminder | ✅ | ✅ | ✅ | — |
| Native game hook (LunaHook) | ✅ | — | — | — |
| Reading overlay | ✅ | — | — | — |
| Texthooker (WebSocket, clipboard) | ✅ | ✅ | ✅ | ✅ |

*Settings → Tracking* shows the same list for the running system.

- **Linux**: Wayland does not let apps see other windows, so players are followed
  through MPRIS. Wine and Proton games are matched by the `.exe` in their command
  line. For text, run Textractor inside the game's prefix and connect it over
  WebSocket.
- **macOS**: window titles need System Settings → Privacy & Security → Screen
  Recording. Without it, novels, games, and focus still work; players are only
  detected through mpv's IPC.

## License

The dictionary engine, Hoshidicts `main`, is licensed under GPL-3.0-only. The
companion links it directly, so releases that include it must use GPL-compatible
terms and provide the corresponding source. Upstream source and license are in
`crates/nihongo-dict/vendor/`; other third-party notices are in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## LLM-assisted development

This application was developed with the help of large language models, and much
of the code has not been manually reviewed. The project aims for high quality
through development guard rails, automated checks, and the Rust test suite, but
use it at your own discretion and review the code before relying on it.
