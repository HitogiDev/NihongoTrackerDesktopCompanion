import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { ipc, events } from '../lib/ipc';
import { setAppLanguage } from '../i18n';
import { detectLanguage } from '../i18n/languages';
import type { ConnectionStatus, HookerState, Settings, TrackerSnapshot, UserSummary } from '../lib/types';

interface AppStore {
  ready: boolean;
  settings: Settings | null;
  tracker: TrackerSnapshot | null;
  hooker: HookerState | null;
  /** `null` while unknown, then the result of the last `auth/verify`. */
  online: boolean | null;
  user: UserSummary | null;
  /** The API key was rejected (401): nothing will work until it is replaced. */
  authRequired: boolean;

  bootstrap: () => Promise<void>;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  /** `cached` lets a window that is starting up reuse the last broadcast status. */
  refreshConnection: (cached?: boolean) => Promise<void>;
  setAuthRequired: (value: boolean) => void;
  setTheme: (theme: string) => void;
}

function applyTheme(theme: string) {
  const resolved =
    theme === 'system'
      ? window.matchMedia('(prefers-color-scheme: light)').matches
        ? 'light'
        : 'dark'
      : theme;
  document.documentElement.setAttribute('data-theme', resolved);
}

// StrictMode mounts effects twice in dev; the event listeners must be attached
// exactly once.
let listenersAttached = false;

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  settings: null,
  tracker: null,
  hooker: null,
  online: null,
  user: null,
  authRequired: false,

  bootstrap: async () => {
    const [settings, tracker, hooker] = await Promise.all([
      ipc.getSettings(),
      ipc.getTrackerState(),
      ipc.hookerState(),
    ]);
    applyTheme(settings.theme);
    void setAppLanguage(settings.language || detectLanguage());
    set({ settings, tracker, hooker, ready: true });

    if (!listenersAttached) {
      listenersAttached = true;
      // Every window gets the snapshot each second. A hidden one keeps only the
      // latest and renders it when shown, instead of re-rendering unseen.
      let hiddenTracker: TrackerSnapshot | null = null;
      await listen<TrackerSnapshot>(events.tracker, (event) => {
        if (document.hidden) hiddenTracker = event.payload;
        else set({ tracker: event.payload });
      });
      document.addEventListener('visibilitychange', () => {
        if (document.hidden || !hiddenTracker) return;
        set({ tracker: hiddenTracker });
        hiddenTracker = null;
      });
      await listen<HookerState>(events.hookerStatus, (event) => set({ hooker: event.payload }));
      await listen<Settings>(events.settings, (event) => {
        applyTheme(event.payload.theme);
        void setAppLanguage(event.payload.language || detectLanguage());
        const previous = get().settings;
        const authChanged = previous?.api_base_url !== event.payload.api_base_url
          || previous?.api_key !== event.payload.api_key;
        set({
          settings: event.payload,
          ...(authChanged ? { online: event.payload.api_key ? null : false, user: null } : {}),
        });
      });
      await listen<ConnectionStatus>(events.connection, (event) => {
        set({ online: event.payload.ok, user: event.payload.ok ? event.payload.user ?? null : null });
        if (event.payload.ok) set({ authRequired: false });
      });
      window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
        if (get().settings?.theme === 'system') applyTheme('system');
      });
    }

    void get().refreshConnection(true);
  },

  updateSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    const saved = await ipc.saveSettings(next);
    applyTheme(saved.theme);
    void setAppLanguage(saved.language || detectLanguage());
    // Saving is the way out of a rejected key, and Rust clears its own flag
    // here too, so the warning goes with it.
    const authChanged = current.api_base_url !== saved.api_base_url || current.api_key !== saved.api_key;
    set({
      settings: saved,
      ...(authChanged ? { online: saved.api_key ? null : false, user: null, authRequired: false } : {}),
    });
  },

  refreshConnection: async (cached = false) => {
    try {
      const status = await ipc.connectionStatus(cached);
      if (status.reason === 'settings-changed') return;
      set({ online: status.ok, user: status.ok ? status.user ?? null : null });
      if (status.ok) set({ authRequired: false });
    } catch {
      set({ online: false, user: null });
    }
  },

  setAuthRequired: (value) => set({ authRequired: value }),

  setTheme: (theme) => {
    applyTheme(theme);
    void get().updateSettings({ theme });
  },
}));
