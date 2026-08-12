import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { ipc, events } from '../lib/ipc';
import type { HookerState, Settings, TrackerSnapshot } from '../lib/types';

interface AppStore {
  ready: boolean;
  settings: Settings | null;
  tracker: TrackerSnapshot | null;
  hooker: HookerState | null;
  /** `null` while unknown, then the result of the last `auth/verify`. */
  online: boolean | null;

  bootstrap: () => Promise<void>;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  refreshConnection: () => Promise<void>;
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

  bootstrap: async () => {
    const [settings, tracker, hooker] = await Promise.all([
      ipc.getSettings(),
      ipc.getTrackerState(),
      ipc.hookerState(),
    ]);
    applyTheme(settings.theme);
    set({ settings, tracker, hooker, ready: true });

    if (!listenersAttached) {
      listenersAttached = true;
      await listen<TrackerSnapshot>(events.tracker, (event) => set({ tracker: event.payload }));
      await listen<HookerState>(events.hookerStatus, (event) => set({ hooker: event.payload }));
      await listen<Settings>(events.settings, (event) => {
        applyTheme(event.payload.theme);
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
    set({ settings: saved });
  },

  refreshConnection: async () => {
    try {
      const status = await ipc.connectionStatus();
      set({ online: status.ok });
    } catch {
      set({ online: false });
    }
  },

  setTheme: (theme) => {
    applyTheme(theme);
    void get().updateSettings({ theme });
  },
}));
