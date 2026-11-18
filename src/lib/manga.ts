import { convertFileSrc } from '@tauri-apps/api/core';

// Pages, thumbnails and covers come from Rust through the `manga://` protocol,
// by opaque id only (see `src-tauri/src/manga/protocol.rs`). `convertFileSrc`
// spells the scheme the way each webview expects it (`http://manga.localhost`
// on Windows, `manga://localhost` elsewhere).
const SCHEME = 'manga';

export const mangaUrl = {
  page: (volumeId: string, page: number) => convertFileSrc(`page/${volumeId}/${page}`, SCHEME),
  thumb: (volumeId: string, page: number) => convertFileSrc(`thumb/${volumeId}/${page}`, SCHEME),
  cover: (seriesId: string) => convertFileSrc(`cover/${seriesId}`, SCHEME),
  /** An image of the extensions server, by the token Rust gave for it. */
  ext: (token: string) => convertFileSrc(`ext/${token}`, SCHEME),
};

/** The i18n key for an error code Rust sends back, if it is one of ours. */
export function mangaErrorKey(error: unknown): string | null {
  const code = String(error);
  if (code === 'not-a-folder') return 'errors.notAFolder';
  if (code === 'already-added') return 'errors.alreadyAdded';
  if (WEBDAV_ERRORS.has(code)) return `webdav.errors.${code}`;
  if (EXTENSION_ERRORS.has(code)) return `ext.errors.${code}`;
  return null;
}

/** Codes only the extensions server sends (`manga::suwayomi::graphql::SwError`). */
const EXTENSION_ERRORS = new Set(['not-installed', 'not-configured', 'start-failed', 'server', 'bad-reply', 'adult-hidden', 'unsupported', 'busy', 'other-server']);

/** The codes `manga::webdav::client::DavError::code` sends. */
const WEBDAV_ERRORS = new Set(['offline', 'unauthorized', 'not-found', 'not-webdav', 'needs-download', 'too-large', 'changed', 'cancelled', 'invalid-url', 'error']);

/** The server's address without the scheme, for display: `files.example/dav/`. */
export function displayUrl(url: string): string {
  return url.replace(/^https?:\/\//, '').replace(/\/$/, '') || url;
}

/** A count with the language's thousands separator: `1,204`, `12.040`. */
export function formatCount(value: number | bigint, locale?: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/** `1.2 GB`, `350 MB`: a size as people read it. */
export function formatBytes(bytes: number, locale?: string): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) { value /= 1000; unit += 1; }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(value)} ${units[unit]}`;
}

/** 0–100 for a progress bar: where the user is, a finished volume in full. */
export function readPercent(page: number | null | undefined, pageCount: number, finished = false): number {
  if (finished) return 100;
  if (page === null || page === undefined || pageCount <= 0) return 0;
  return Math.round((Math.min(page + 1, pageCount) / pageCount) * 100);
}

/** "5 minutes ago", "yesterday": how long since `iso`, in the user's language. */
export function relativeTime(iso: string, locale?: string, now = Date.now()): string {
  const seconds = Math.round((Date.parse(iso) - now) / 1000);
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const steps: [Intl.RelativeTimeFormatUnit, number][] = [['second', 60], ['minute', 60], ['hour', 24], ['day', 30], ['month', 12], ['year', Infinity]];
  let value = seconds;
  for (const [unit, size] of steps) {
    if (Math.abs(value) < size) return format.format(unit === 'second' && Math.abs(value) < 10 ? 0 : value, unit);
    value = Math.round(value / size);
  }
  return format.format(value, 'year');
}
