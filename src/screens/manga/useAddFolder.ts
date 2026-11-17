import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';
import { ipc } from '../../lib/ipc';
import { mangaErrorKey } from '../../lib/manga';
import { toast } from '../../store/useToastStore';

/** Picks a folder and adds it as a library root; Rust scans it right away. */
export function useAddFolder() {
  const { t } = useTranslation('manga');
  const [adding, setAdding] = useState(false);
  const add = async () => {
    const picked = await open({ directory: true, multiple: false, title: t('folders.pick') });
    if (typeof picked !== 'string' || !picked) return;
    setAdding(true);
    try {
      const summary = await ipc.mangaAddLocalRoot(picked);
      if (summary.series === 0) toast.warning(t('toast.addedEmpty'));
      else toast.success(t('toast.added', { count: summary.series }));
    } catch (error) {
      const key = mangaErrorKey(error);
      toast.error(key ? t(key) : t('toast.addFailed', { message: String(error) }));
    } finally {
      setAdding(false);
    }
  };
  return { add, adding };
}

/** Rescans the roots; `quiet` for the automatic scan when the screen opens. */
export function useRescan() {
  const { t } = useTranslation('manga');
  const [scanning, setScanning] = useState(false);
  const rescan = async (quiet = false) => {
    if (!quiet) setScanning(true);
    try {
      const summary = await ipc.mangaRescan(!quiet);
      if (quiet || summary.skipped) return;
      if (summary.offline.length) toast.warning(t('toast.offline', { names: summary.offline.join(', ') }));
      else toast.success(t('toast.rescanned', { count: summary.series }));
    } catch (error) {
      if (!quiet) toast.error(t('toast.rescanFailed', { message: String(error) }));
    } finally {
      setScanning(false);
    }
  };
  return { rescan, scanning };
}
