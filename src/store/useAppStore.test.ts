import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '../lib/types';

// Rust is not there: the IPC layer and the event bus are stand-ins the tests drive.
const handlers = new Map<string, (event: { payload: unknown }) => void>();
const ipc = vi.hoisted(() => ({
  getSettings: vi.fn(),
  getTrackerState: vi.fn(),
  hookerState: vi.fn(),
  saveSettings: vi.fn(),
  connectionStatus: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  }),
}));
vi.mock('../lib/ipc', async () => {
  const actual = await vi.importActual<typeof import('../lib/ipc')>('../lib/ipc');
  return { ...actual, ipc };
});
vi.mock('../i18n', () => ({ setAppLanguage: vi.fn() }));

const { useAppStore } = await import('./useAppStore');
const { events } = await import('../lib/ipc');

const settings = (patch: Partial<Settings> = {}) =>
  ({ theme: 'dark', language: 'en', api_base_url: 'https://nihongotracker.app', api_key: 'key', ...patch }) as Settings;

function emit(name: string, payload: unknown) {
  const handler = handlers.get(name);
  if (!handler) throw new Error(`nobody listens to ${name}`);
  handler({ payload });
}

beforeEach(() => {
  window.matchMedia = vi.fn(() => ({ matches: false, addEventListener: vi.fn() })) as unknown as typeof window.matchMedia;
  vi.clearAllMocks();
  ipc.getSettings.mockResolvedValue(settings());
  ipc.getTrackerState.mockResolvedValue({ pending_count: 0 });
  ipc.hookerState.mockResolvedValue({ status: 'disconnected' });
  ipc.connectionStatus.mockResolvedValue({ ok: true, user: { username: 'hitogi' } });
  useAppStore.setState({ ready: false, settings: null, online: null, user: null, authRequired: false });
});

describe('bootstrap', () => {
  it('loads everything, applies the theme and asks for the cached connection', async () => {
    await useAppStore.getState().bootstrap();

    const state = useAppStore.getState();
    expect(state.ready).toBe(true);
    expect(state.settings?.theme).toBe('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(ipc.connectionStatus).toHaveBeenCalledWith(true);
    await vi.waitFor(() => expect(useAppStore.getState().online).toBe(true));
    expect(useAppStore.getState().user?.username).toBe('hitogi');
  });

  it('attaches the event listeners only once', async () => {
    const { listen } = await import('@tauri-apps/api/event');
    await useAppStore.getState().bootstrap();
    const calls = vi.mocked(listen).mock.calls.length;
    await useAppStore.getState().bootstrap();
    expect(vi.mocked(listen).mock.calls.length).toBe(calls);
  });
});

describe('settings events', () => {
  beforeEach(async () => {
    await useAppStore.getState().bootstrap();
    await vi.waitFor(() => expect(useAppStore.getState().online).toBe(true));
  });

  it('forgets the connection when the key changes elsewhere', () => {
    emit(events.settings, settings({ api_key: 'other' }));
    expect(useAppStore.getState().online).toBeNull();
    expect(useAppStore.getState().user).toBeNull();
  });

  it('knows it is offline when the key is removed', () => {
    emit(events.settings, settings({ api_key: null }));
    expect(useAppStore.getState().online).toBe(false);
  });

  it('keeps the connection for unrelated changes', () => {
    emit(events.settings, settings({ theme: 'light' }));
    expect(useAppStore.getState().online).toBe(true);
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });

  it('a good connection clears the rejected-key warning', () => {
    useAppStore.getState().setAuthRequired(true);
    emit(events.connection, { ok: true, user: { username: 'hitogi' } });
    expect(useAppStore.getState().authRequired).toBe(false);
  });
});

describe('updateSettings', () => {
  beforeEach(async () => {
    await useAppStore.getState().bootstrap();
  });

  it('saves the merged settings and keeps what Rust returned', async () => {
    ipc.saveSettings.mockImplementation(async (next: Settings) => ({ ...next, theme: 'light' }));
    await useAppStore.getState().updateSettings({ theme: 'nord' });
    expect(ipc.saveSettings).toHaveBeenCalledWith(expect.objectContaining({ theme: 'nord', api_key: 'key' }));
    expect(useAppStore.getState().settings?.theme).toBe('light');
  });

  it('a new key clears the rejected-key warning', async () => {
    useAppStore.getState().setAuthRequired(true);
    ipc.saveSettings.mockImplementation(async (next: Settings) => next);
    await useAppStore.getState().updateSettings({ api_key: 'fresh' });
    expect(useAppStore.getState().authRequired).toBe(false);
    expect(useAppStore.getState().online).toBeNull();
  });
});

describe('refreshConnection', () => {
  it('ignores a status from before a settings change', async () => {
    useAppStore.setState({ online: true });
    ipc.connectionStatus.mockResolvedValue({ ok: false, reason: 'settings-changed' });
    await useAppStore.getState().refreshConnection();
    expect(useAppStore.getState().online).toBe(true);
  });

  it('goes offline when the call fails', async () => {
    useAppStore.setState({ online: true, user: { username: 'x' } });
    ipc.connectionStatus.mockRejectedValue(new Error('down'));
    await useAppStore.getState().refreshConnection();
    expect(useAppStore.getState().online).toBe(false);
    expect(useAppStore.getState().user).toBeNull();
  });
});
