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
};

/** The i18n key for an error code Rust sends back, if it is one of ours. */
export function mangaErrorKey(error: unknown): string | null {
  const code = String(error);
  if (code === 'not-a-folder') return 'errors.notAFolder';
  if (code === 'already-added') return 'errors.alreadyAdded';
  return null;
}

/** 0–100 for a progress bar: where the user is, a finished volume in full. */
export function readPercent(page: number | null | undefined, pageCount: number, finished = false): number {
  if (finished) return 100;
  if (page === null || page === undefined || pageCount <= 0) return 0;
  return Math.round((Math.min(page + 1, pageCount) / pageCount) * 100);
}
