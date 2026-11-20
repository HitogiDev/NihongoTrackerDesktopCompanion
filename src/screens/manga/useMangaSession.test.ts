import { describe, expect, it } from 'vitest';
import type { MangaSession } from '../../lib/types';
import { sessionAt } from './useMangaSession';

const session = (over: Partial<MangaSession> = {}): MangaSession => ({
  volume_id: 'v1',
  series_id: 's1',
  series_title: 'S',
  volume_title: 'V',
  media_id: null,
  media_title: null,
  is_adult: false,
  has_text: false,
  page: 0,
  page_count: 10,
  pages_read: 0,
  chars_read: 0,
  elapsed_secs: 100,
  counting: true,
  pause: null,
  inactive_secs: 0,
  started_at: 't',
  ...over,
});

describe('sessionAt', () => {
  it('runs a counting clock on between the sessions Rust sends', () => {
    expect(sessionAt(session(), 1_000, 1_999)?.elapsed_secs).toBe(100);
    expect(sessionAt(session(), 1_000, 8_500)?.elapsed_secs).toBe(107);
  });

  it('leaves a stopped clock, and no session, as they are', () => {
    expect(sessionAt(session({ counting: false }), 1_000, 60_000)?.elapsed_secs).toBe(100);
    expect(sessionAt(null, 1_000, 60_000)).toBeNull();
    expect(sessionAt(session(), 5_000, 1_000)?.elapsed_secs).toBe(100);
  });
});
