import { describe, expect, it } from 'vitest';
import { displayUrl, formatBytes, isServedFormat, mangaErrorKey, readPercent, relativeTime } from './manga';

describe('manga helpers', () => {
  it('formats sizes as people read them', () => {
    expect(formatBytes(512, 'en')).toBe('512 B');
    expect(formatBytes(37_043_935, 'en')).toBe('37 MB');
    expect(formatBytes(2_000_000_000, 'en')).toBe('2 GB');
    expect(formatBytes(1_250_000, 'es')).toBe('1,3 MB');
  });

  it('shows a server address without its scheme', () => {
    expect(displayUrl('https://files.example/dav/')).toBe('files.example/dav');
    expect(displayUrl('http://127.0.0.1:8080/')).toBe('127.0.0.1:8080');
  });

  it('says how long ago something was', () => {
    const now = Date.parse('2026-11-17T12:00:00Z');
    expect(relativeTime('2026-11-17T11:59:58Z', 'en', now)).toBe('now');
    expect(relativeTime('2026-11-17T11:55:00Z', 'en', now)).toBe('5 minutes ago');
    expect(relativeTime('2026-11-16T12:00:00Z', 'en', now)).toBe('yesterday');
  });

  it('maps WebDAV error codes to their sentences', () => {
    expect(mangaErrorKey('offline')).toBe('webdav.errors.offline');
    expect(mangaErrorKey('needs-download')).toBe('webdav.errors.needs-download');
    expect(mangaErrorKey('already-added')).toBe('errors.alreadyAdded');
    expect(mangaErrorKey('something else')).toBeNull();
  });

  it('keeps read percentages in range', () => {
    expect(readPercent(9, 10)).toBe(100);
    expect(readPercent(null, 10)).toBe(0);
  });

  it('tells volumes served from a server from ones on disk', () => {
    expect(['remote-archive', 'remote-folder', 'suwayomi-chapter'].every(isServedFormat)).toBe(true);
    expect(['archive', 'folder'].some(isServedFormat)).toBe(false);
  });
});
