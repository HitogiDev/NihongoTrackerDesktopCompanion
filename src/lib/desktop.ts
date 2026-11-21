// Browser behaviour that gives a webview away inside a desktop window. WebView2
// keeps its reload, print, find and caret-browsing keys and its own context menu
// ("Back / Reload / Inspect") unless the page swallows them; Tauri 2.11 does not
// expose `AreBrowserAcceleratorKeysEnabled`. Development builds keep the keys
// (reload is handy there) and, with Shift held, the context menu (Inspect).

const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"], .selectable';

function isEditable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE) !== null;
}

/** Shortcuts that only make sense in a browser tab. */
function isBrowserShortcut(event: KeyboardEvent): boolean {
  const key = event.key.toLowerCase();
  const mod = event.ctrlKey || event.metaKey;
  if (['f3', 'f5', 'f7', 'browserrefresh', 'browserback', 'browserforward', 'browsersearch'].includes(key)) return true;
  if (event.altKey && (key === 'arrowleft' || key === 'arrowright')) return true;
  // Ctrl+F stays available to inputs that want it; anywhere else it opens
  // WebView2's find bar.
  if (mod && key === 'f') return !isEditable(event.target);
  return mod && ['r', 'p', 's', 'u', 'g', 'j', 'h', 'o'].includes(key);
}

export function installDesktopBehavior() {
  // The webview's menu never shows: it is a web page's ("Back / Reload"),
  // and in development it brings "Inspect" even into text fields. A right
  // click is the app's own menu where it has one: `useContextMenu` on its
  // items, `TextMenu` on text. In development Shift+right click lets it
  // through, for Inspect.
  window.addEventListener('contextmenu', (event) => {
    if (import.meta.env.DEV && event.shiftKey) return;
    event.preventDefault();
  });

  if (import.meta.env.DEV) return;

  window.addEventListener('keydown', (event) => {
    if (isBrowserShortcut(event)) event.preventDefault();
  }, { capture: true });

  // Dragging an image or a link out of the window would hand a file URL or a
  // tauri:// address to whatever it is dropped on.
  window.addEventListener('dragstart', (event) => {
    if (event.target instanceof HTMLImageElement || event.target instanceof HTMLAnchorElement) event.preventDefault();
  });
}

/** "Ctrl+1" on Windows and Linux, "⌘1" on macOS. */
export function shortcutLabel(key: string): string {
  return /mac/i.test(navigator.platform) ? `⌘${key}` : `Ctrl+${key}`;
}
