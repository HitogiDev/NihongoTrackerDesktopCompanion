import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { create } from 'zustand';
import i18n from '../../i18n';
import { events, ipc } from '../../lib/ipc';
import { mangaErrorKey } from '../../lib/manga';
import type { DownloadProgress, WebdavSyncProgress } from '../../lib/types';
import { toast } from '../../store/useToastStore';

interface SyncStore {
  /** The last progress Rust reported for each source being listed. */
  progress: Record<string, WebdavSyncProgress>;
  set: (progress: WebdavSyncProgress) => void;
}

export const useWebdavSyncStore = create<SyncStore>((set) => ({
  progress: {},
  set: (progress) => set((state) => ({ progress: { ...state.progress, [progress.source_id]: progress } })),
}));

/** Keeps `useWebdavSyncStore` fed from `manga://webdav-sync`. Once per screen. */
export function useWebdavSyncEvents() {
  const setProgress = useWebdavSyncStore((s) => s.set);
  useEffect(() => {
    const off = listen<WebdavSyncProgress>(events.mangaWebdavSync, (event) => setProgress(event.payload));
    return () => void off.then((fn) => fn());
  }, [setProgress]);
}

/** A WebDAV error code as a sentence, for toasts and the form. */
export function webdavErrorText(error: unknown): string {
  const key = mangaErrorKey(error);
  return key ? i18n.t(`manga:${key}`) : i18n.t('manga:webdav.errors.error');
}

/** Lists a source again from a button; errors become a toast. */
export async function syncSource(sourceId: string, force = false) {
  try {
    await ipc.mangaWebdavSync(sourceId, force);
  } catch (error) {
    toast.error(i18n.t('manga:webdav.toast.syncFailed', { message: webdavErrorText(error) }));
  }
}

interface DownloadStore {
  /** Downloads running now, by volume id. */
  running: Record<string, DownloadProgress>;
  update: (progress: DownloadProgress) => void;
  reset: (list: DownloadProgress[]) => void;
}

export const useDownloadStore = create<DownloadStore>((set) => ({
  running: {},
  update: (progress) => set((state) => {
    const running = { ...state.running };
    if (progress.done) delete running[progress.volume_id];
    else running[progress.volume_id] = progress;
    return { running };
  }),
  reset: (list) => set({ running: Object.fromEntries(list.map((item) => [item.volume_id, item])) }),
}));

/** Keeps `useDownloadStore` fed: what runs now, then `manga://download`. */
export function useDownloadEvents() {
  const update = useDownloadStore((s) => s.update);
  const reset = useDownloadStore((s) => s.reset);
  useEffect(() => {
    void ipc.mangaDownloads().then(reset).catch(() => undefined);
    const off = listen<DownloadProgress>(events.mangaDownload, (event) => update(event.payload));
    return () => void off.then((fn) => fn());
  }, [update, reset]);
}

/** Downloads a volume for offline reading, with a toast at the end. */
export async function downloadForOffline(volumeId: string, title: string) {
  try {
    await ipc.mangaDownloadVolume(volumeId, true);
    toast.success(i18n.t('manga:webdav.toast.downloaded', { title }));
  } catch (error) {
    if (String(error) === 'cancelled') return;
    toast.error(i18n.t('manga:webdav.toast.downloadFailed', { message: webdavErrorText(error) }));
  }
}
