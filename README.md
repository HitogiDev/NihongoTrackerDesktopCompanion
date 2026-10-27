# NihongoTracker Desktop Companion

Desktop companion for [NihongoTracker](https://github.com/HitogiDev/NihongoTracker).
It detects what you are watching or reading and logs it automatically, similar to
[Taiga](https://github.com/erengy/taiga), but for Japanese immersion.

- **Automatic anime tracking.** Recognizes mpv, mpv.net, MPC-HC/BE, VLC,
  PotPlayer, GOM, KMPlayer, and SMPlayer. It extracts the title and episode from
  the filename, searches NihongoTracker, and logs the episode after the viewing
  threshold is reached. With mpv and MPC, it uses the video's actual position
  instead of counting time while the window is open, so paused video does not
  count.
- **Visual novels.** Link an `.exe` to a title. Opening it starts a session,
  opens the texthooker, and counts time only while the window is active. You do
  not need to pause or resume anything manually.
- **Games.** Link a game's executable in the library. The companion counts time
  only while the game is focused and you are not AFK, then places the session in
  Pending for review. Games do not open the texthooker automatically, but you
  can open it from the active session or the sidebar.
- **Sentence mining.** Save any texthooker line or press Ctrl+Shift+M (⌘⇧M on
  macOS) to save the latest line while another window is active. The shortcut
  also captures the focused linked game or novel window when the system allows
  it (Windows, Linux on X11, macOS with Screen Recording). Sentences,
  work titles, and captures are kept locally. The Sentences screen can send
  them to Anki Desktop through AnkiConnect.
- **Floating session panel.** An optional always-on-top window shows the active
  work, timer, latest texthooker line, and controls to pause, mine a sentence,
  open the texthooker, or create a quick log. Enable it in Settings.
- **NihongoTracker's texthooker inside the app.** A close port of `HookerScreen`
  with the same floating bar and metrics, a wall-clock anchored timer with an
  inactivity pause, the same fonts and themes, custom CSS using the same classes
  (`.th-root`, `.th-topbar`, `.th-line-text`, and others), vertical text, line
  deletion, and undo. It includes a WebSocket client for Textractor / Agent with
  the same character counting rules, counting kana and kanji only, plus a
  clipboard reader that replaces the browser extension. It works offline:
  everything is stored in SQLite and synchronized when the network returns.
- **Offline-first operation.** Every log goes through an `outbox` queue with
  retries, so closing the app, losing the network, or restarting the router does
  not lose data.
- **Streak reminder.** If the end of your day is approaching and you have not
  logged anything, the app sends one notification per day after checking the
  server again. A button opens the log form on supported systems. See below.
- **Offline dictionary support.** Import Yomitan-style dictionary zips and look
  up expressions from the texthooker without a browser or network connection.
  See below.
- **Optional Discord Rich Presence.** Disabled by default. It can show what you
  are reading or watching with cover art and progress, while never showing the
  title of adult media. See below.
- **English and Spanish.** Choose the language in Settings or during onboarding.
  The default language follows the operating system.
- **Optional confirmation.** By default, detections wait for approval in
  *Pending*, with XP estimated by the server. You can also log the current
  episode manually without waiting for the threshold.

## Stack

| Layer | Choice | Reason |
|---|---|---|
| Runtime | Tauri v2 (Rust + system WebView) | About a 15 MB installer and tens of MB of RAM, rather than hundreds like Electron |
| UI | React 19 + Vite 7 + Tailwind v4 + daisyUI 5 | Shares the design system and 19 themes used by NihongoTracker's frontend |
| Local data | SQLite (`rusqlite`, bundled) | Offline queue, texthooker lines, and title registry |
| Network | `reqwest` from Rust | Avoids the backend CORS allowlist, which only includes the web domain |

## How it works internally

```
src-tauri/src/
  platform/      OS-specific windows, focus, idle time and media players (Windows, Linux, macOS)
  players.rs     Known players and window-title cleanup
  parser.rs      Anitomy-style release-name parser (with tests)
  watcher.rs     1-second loop for anime and VN sessions, including log completion
  texthooker.rs  WebSocket client (ws://localhost:6677) and clipboard reader
  api.rs         REST client using the x-api-key header
  sync.rs        Outbox retries and line synchronization
  db.rs          SQLite schema and queries
  commands.rs    IPC surface consumed by the frontend
src/
  screens/       Activity, Pending, Library, Texthooker, and Settings
  components/ui/ Button, Modal, Field, Page, and other components copied from the web design system
```

### Authentication

The backend accepts `x-api-key` before the JWT cookie
(`Backend/src/middlewares/authMiddleware.ts`). The cookie is `httpOnly` and
`sameSite: strict`, so it cannot be used by a native app. The companion asks for
an API key, available under Settings -> API keys on the website, in `ntk_...`
format, and stores it in the local SQLite database.

### Anime detection

1. The app lists visible windows and looks for a known media-player process.
2. It removes the player's marker from the title, such as `... - mpv`, to get
   the filename.
3. `parser.rs` extracts the title, episode, season, and release group.
4. The title is resolved through `GET /api/media/search?type=anime`, using an
   in-memory cache and a manual-link table that takes priority.
5. When the threshold is reached, the log is sent or placed in **Pending**,
   depending on the settings.

#### Determining episode progress

Time with the window open is the fallback, so paused video can still count. When
the player exposes its position, the companion queries it instead:

| Player | Method | Required setting |
|---|---|---|
| mpv, mpv.net | JSON IPC over a named pipe or socket | Set `input-ipc-server=/tmp/mpv-socket` in `mpv.conf`. On Windows, mpv converts this to `\\.\pipe\tmp\mpv-socket`, and the companion performs the same translation |
| MPC-HC, MPC-BE | `variables.html` from the web interface | Options -> Player -> Web interface -> enable *Listen on port* 13579 |
| Other players | Elapsed time | None |

With actual position data, progress is `position / duration`, and pausing no
longer adds time. Change the socket path and port under Settings -> Anime.

### Visual novel detection

When a registered process appears, the companion starts the session, assigns the
texthooker to that title, and opens the screen. The timer runs only while the
window is in the foreground, if configured, and while the user is not idle past
the AFK limit. When the process closes, the companion creates a log with the
session minutes and texthooker character count.

### Game tracking

Choose Game when adding an executable in the library and link it to a game entry
on NihongoTracker. The game detector uses the same process watcher as visual
novels, but requires the game to be in the foreground and never starts the
texthooker on its own. Open it from the active session or sidebar when you want
to use a hooker with the game. The session is saved to Pending on exit, where
you can review its measured minutes before sending it. The AFK timeout and
manual pause also apply. For supported games, enable **Capture game text with
LunaHook** on the game's library entry. The companion chooses the x86 or x64
helper to match the game and sends captured Japanese text to its built-in
texthooker. This feature is Windows-only. Building a release with both helpers
requires C++ build tools for x86 and x64.

In the texthooker, click **LunaHook** to open the hook selector. It also opens
automatically when LunaHook starts for a game without a saved choice. Advance
the dialogue to see live samples, then select a source and click **Save hook**.
The choice is saved per executable and reused on the next launch. Unselected
sources remain visible in the selector, but their text is not added to the reader.
The selector shows attachment errors and can enable LunaHook for the active
registered game.

### Sentence mining and overlay

Use the bookmark button beside a texthooker line, the reader toolbar, the
floating panel, or Ctrl+Shift+M to save a sentence. A line is saved once, so
repeating the shortcut does not create duplicate cards. The local copy stays
available without an account or network. Anki export sends the sentence to the
Front field and the work title and optional screenshot to the Back field of a
Basic note via AnkiConnect at `127.0.0.1:8765`. Anki and the add-on must be
running. Audio clips are not part of this integration.

### VN reading overlay

Edit a VN in Library and choose **Reader -> Overlay**. This enables LunaHook
and replaces the automatic reader window with a transparent dialogue overlay.
Integrated and Browser remain available, and existing reader choices are preserved.
The separate session statistics panel keeps its current behavior.

Hover over a Japanese word to open the local dictionary. The popup stays in a
separate window and keeps the consulted sentence while new dialogue arrives.
Its **+** button sends the word, reading, meaning, original sentence, and work
title through the Anki field mapping in Settings. AnkiConnect must be running.
Dictionary lookup works offline with imported dictionaries.

The toolbar appears on hover. Use it to choose and save a LunaHook source,
move the overlay, open its settings window, let clicks pass through, or hide it.
Drag the bottom-right grip to resize it. Position and size survive app restarts.

Open **Overlay settings** from the overlay toolbar or from **Settings -> Tracking**.
Its separate window controls font weight, alignment, background opacity,
hover visibility, idle opacity, toolbar visibility, lookup delay, window priority,
and global shortcuts. The defaults are **Ctrl+Shift+O** to show or hide the
overlay and **Ctrl+Shift+I** to toggle click-through. Shortcut conflicts appear
there. These controls also let you recover mouse interaction if a shortcut fails.

The overlay follows the active VN process, stays visible while its settings window
has focus, and hides when another app is foreground. It closes its popup when the
game ends. It does not change the selected reader session, game focus, timer,
capture source, or synchronization behavior. Opening
the texthooker manually still opens the integrated reader for an Overlay VN.

This version uses captured text, without OCR. Windows games in windowed or
borderless mode are the target. Exclusive fullscreen rendering is not supported.

### Streak reminder

The reminder uses the time zone configured for your NihongoTracker account, not
the time zone of this computer. If you travel or the system time zone is wrong,
the reminder still follows the correct account day. It appears when fewer than X
hours remain before midnight, three by default and configurable, and can also be
scheduled for a fixed time.

To avoid unnecessary notifications:

- The app sends at most one notification per day, even if you close and reopen
  it.
- It checks the server immediately before notifying. A log created from the web
  or mobile app a minute earlier cancels the reminder without showing anything.
- If the app was closed and you open it after the reminder time, it does not show
  old reminders.
- The notification has a "Log now" button on Windows and Linux. On macOS it only
  informs you; open the form from the tray or from *Activity* instead.

Disable the reminder under Settings -> Application, along with the fixed time and
notification hours. A log with an unknown date does not count toward the server
streak, so the reminder checks `lastStreakDate` rather than the logs you sent.

### Discord Rich Presence

Discord integration is optional and **disabled by default**. Until you enable it,
the app does not publish anything or open a Discord connection.

You need your own application in the
[Discord Developer Portal](https://discord.com/developers/applications). Copy
its *Application ID* into Settings -> Discord Rich Presence and upload your icons
under *Rich Presence -> Art Assets*. Configure the fallback asset name there as
well. Discord does not need to be open when the app starts. If it is closed, the
companion retries automatically.

What can be published:

| | |
|---|---|
| Line 1 | `Reading 蒼の彼方のフォーリズム` or `Watching Frieren` |
| Line 2 | `12.4k characters` or `Episode 3` |
| Time | From the start of the session, counted by Discord |
| Image | The title's cover art or your fallback icon |
| Button | `View profile`, linking to your public profile. You can remove it |

The following is never published:

- The title or cover art of media marked as adult in NihongoTracker. It falls
  back to the generic status `Japanese immersion`.
- The filename when the title could not be identified.
- Anything on your blacklist, whether blacklisted by title or executable. No
  generic activity is shown for it.
- Anything while the session is paused, after the app closes, or after you turn
  off the toggle. The presence is cleared instead of remaining stuck on the last
  session.

There is also an **activity-only** mode that hides every title. Settings includes
a live preview of the exact presence that would be published at that moment. The
preview uses the same function as the publishing logic, so you can check it
before enabling the feature.

### Offline Yomitan-style dictionaries

The texthooker can import a dictionary `.zip` exported for Yomitan. Go to
**Settings -> Offline dictionaries -> Import dictionary**, select the zip, and
wait for indexing to finish. The file is stored in the app's local data, so
searches do not need a connection or account.

In the texthooker, select a Japanese expression and double-click it, or use the
book button to search the last line. The popup shows entries from every imported
dictionary and lets you follow internal references without opening a browser.

The importer reads `index.json` and every `term_bank_*.json` file in the zip.
Text glosses and structured Yomitan content are rendered safely inside the
popup. Imported content cannot inject HTML, global styles, or external links.

The native engine is [Hoshidicts main](https://github.com/Manhhao/hoshidicts),
not the `main-mit` branch. This provides the current Yomitan-compatible
importer, lookup strategy, frequency and pitch metadata, and Yomitan Japanese
deinflection rules. The companion keeps the converted dictionary data locally,
so lookups remain available without an account or network access.

Hoshidicts main is licensed under GPL-3.0-only. Because the companion links the
engine directly, releases containing this integration must use GPL-compatible
terms and provide the corresponding source under the GPL obligations. The
upstream source and its license are tracked at
`crates/nihongo-dict/vendor/`. Review that licensing decision before publishing
the repository or distributing installers.

## Development

Requirements: Node 20 or newer, stable Rust, CMake, Ninja, and on Windows the
WebView2 Runtime and MSVC Build Tools. Hoshidicts main is included as a git
submodule, so a fresh checkout must initialize it recursively:

```bash
git submodule update --init --recursive
```

```bash
npm install
npm run tauri dev      # app in development mode
npm run build          # frontend bundle only
npm run tauri build    # installer (NSIS on Windows)
cd src-tauri && cargo test   # parser and player tests
```

### Releases and updates

Installed copies check GitHub for a newer release on start and every six hours,
and install it when the user confirms (Settings → App). To publish one, bump
`version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and
`package.json`, commit, and push a matching tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

`.github/workflows/release.yml` builds and signs the installers for the three
systems and publishes them with the `latest.json` manifest the app reads. It
needs the repository secret `TAURI_SIGNING_PRIVATE_KEY` with the private key
that matches `plugins.updater.pubkey`.

The Hoshidicts submodule follows its upstream `main` branch. Updating it is an
intentional dependency update and requires rebuilding the native crate.

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
| Texthooker (WebSocket, clipboard) | ✅ | ✅ | ✅ | ✅ |

*Settings → Tracking* shows the same list for the running system. Implementation:
`platform/windows_impl.rs` (Win32), `linux_impl.rs` (X11 through `x11rb`) and
`dbus.rs` (MPRIS and compositor idle time), `macos_impl.rs` (CoreGraphics).

- **Linux**: Wayland does not let apps see other windows, so players are followed
  through MPRIS (mpv needs the [mpv-mpris](https://github.com/hoyon/mpv-mpris)
  plugin). Wine and Proton games are matched by the `.exe` in their command line.
  For text, run Textractor inside the game's prefix and connect it over WebSocket.
- **macOS**: window titles need System Settings → Privacy & Security → Screen
  Recording. Without it, novels, games and focus still work; players are only
  detected through mpv's IPC.

Linux builds need the usual Tauri libraries (`libwebkit2gtk-4.1-dev`,
`libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`). The CI workflow
in `.github/workflows/ci.yml` builds and tests on all three systems.

## LLM-assisted coding and quality assurance

This application was developed with assistance from large language models. Use
it at your own risk and discretion.

A lot of the code has not been manually verified by humans. The project still
aims for a high level of quality through development guard rails, automated
checks, and the Rust test suite described above. Review the code and decide
whether it is appropriate for your use before relying on it.
