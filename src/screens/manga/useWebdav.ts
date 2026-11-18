import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { create } from 'zustand';
import i18n from '../../i18n';
import { events, ipc } from '../../lib/ipc';
import { mangaErrorKey } from '../../lib/manga';
import type { WebdavSyncProgress } from '../../lib/types';
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
