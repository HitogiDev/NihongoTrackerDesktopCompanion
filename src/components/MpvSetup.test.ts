import { describe, expect, it, vi } from 'vitest';
import type { AnimeSession, Settings } from '../lib/types';

vi.mock('../lib/ipc', () => ({ ipc: {} }));
const { needsMpvSetup } = await import('./MpvSetup');

const session = (patch: Partial<AnimeSession> = {}) =>
  ({ player: 'mpv', source: 'elapsed', elapsed_secs: 30, ...patch }) as AnimeSession;
const settings = (patch: Partial<Settings> = {}) =>
  ({ use_player_position: true, mpv_setup_dismissed: false, ...patch }) as Settings;

describe('needsMpvSetup', () => {
  it('offers the setup while mpv gives no position', () => {
    expect(needsMpvSetup(session(), settings())).toBe(true);
    expect(needsMpvSetup(session({ player: 'mpv.net' }), settings())).toBe(true);
  });

  it('waits for the first probes before deciding mpv is silent', () => {
    expect(needsMpvSetup(session({ elapsed_secs: 2 }), settings())).toBe(false);
  });

  it('stays quiet when the position arrives, for other players, or when told to', () => {
    expect(needsMpvSetup(session({ source: 'player' }), settings())).toBe(false);
    expect(needsMpvSetup(session({ player: 'VLC' }), settings())).toBe(false);
    expect(needsMpvSetup(session(), settings({ mpv_setup_dismissed: true }))).toBe(false);
    expect(needsMpvSetup(session(), settings({ use_player_position: false }))).toBe(false);
    expect(needsMpvSetup(session(), null)).toBe(false);
  });
});
