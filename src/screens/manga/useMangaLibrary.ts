import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import { toast } from '../../store/useToastStore';
import i18n from '../../i18n';
import { mangaErrorKey } from '../../lib/manga';
import { webdavErrorText } from './useWebdav';

export const LIBRARY_KEY = ['manga', 'library'] as const;
export const seriesKey = (id: string) => ['manga', 'series', id] as const;

/**
 * The library, kept fresh by Rust's `manga://library-changed` (a scan, a link,
 * a root removed). Rust scans; this only listens and asks again.
 */
export function useMangaLibraryEvents() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const off = listen(events.mangaLibrary, () => {
      void queryClient.invalidateQueries({ queryKey: ['manga'] });
    });
    return () => void off.then((fn) => fn());
  }, [queryClient]);
}

export function useMangaLibrary() {
  return useQuery({ queryKey: LIBRARY_KEY, queryFn: ipc.mangaLibrary });
}

export function useMangaSeries(id: string) {
  return useQuery({ queryKey: seriesKey(id), queryFn: () => ipc.mangaSeriesDetail(id) });
}

/**
 * Opens the reader window on a volume, at `page` or where the user left off.
 * A remote volume on a server that cannot send parts of files is downloaded
 * first (into the cache, with progress), then opened.
 */
export async function openVolume(volumeId: string, page?: number) {
  try {
    await ipc.mangaOpenReader(volumeId, page);
  } catch (error) {
    if (String(error) === 'needs-download') {
      toast.info(i18n.t('manga:webdav.toast.downloadingToRead'));
      try {
        await ipc.mangaDownloadVolume(volumeId, false);
        await ipc.mangaOpenReader(volumeId, page);
      } catch (inner) {
        if (String(inner) !== 'cancelled') toast.error(i18n.t('manga:toast.openFailed', { message: webdavErrorText(inner) }));
      }
      return;
    }
    const key = mangaErrorKey(error);
    toast.error(i18n.t('manga:toast.openFailed', { message: key ? i18n.t(`manga:${key}`) : String(error) }));
  }
}
