import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { ipc, events } from '../lib/ipc';
import { setAppLanguage } from '../i18n';
import { detectLanguage } from '../i18n/languages';
import type { HookerState, Settings, TrackerSnapshot, UserSummary } from '../lib/types';

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
  refreshConnection: () => Promise<void>;
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
      await listen<TrackerSnapshot>(events.tracker, (event) => set({ tracker: event.payload }));
      await listen<HookerState>(events.hookerStatus, (event) => set({ hooker: event.payload }));
      await listen<Settings>(events.settings, (event) => {
        applyTheme(event.payload.theme);
        void setAppLanguage(event.payload.language || detectLanguage());
        set({ settings: event.payload });
      });
    }

    void get().refreshConnection();
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
    set({ settings: saved, authRequired: false });
  },

  refreshConnection: async () => {
    try {
      const status = await ipc.connectionStatus();
      // `auth/verify` answers { valid, user }, wrapped again by the command.
      set({ online: status.ok, user: status.user?.user ?? null });
      if (status.ok) set({ authRequired: false });
    } catch {
      set({ online: false });
    }
  },

  setAuthRequired: (value) => set({ authRequired: value }),

  setTheme: (theme) => {
    applyTheme(theme);
    void get().updateSettings({ theme });
  },
}));
