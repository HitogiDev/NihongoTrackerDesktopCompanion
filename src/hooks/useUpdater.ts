import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { events, ipc } from '../lib/ipc';
import type { UpdateInfo, UpdateProgress } from '../lib/types';

export const RELEASES_URL = 'https://github.com/HitogiDev/NihongoTrackerDesktopCompanion/releases/latest';

/**
 * The update the Rust side knows about, plus the check and install actions.
 * Rust owns the schedule and the downloaded update; every window just mirrors
 * it, so the banner and Settings agree.
 */
export function useUpdater() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<{ kind: 'check' | 'install'; message: string } | null>(null);

  useEffect(() => {
    void ipc.pendingUpdate().then((found) => { if (found) setUpdate(found); }).catch(() => undefined);
    const offAvailable = listen<UpdateInfo>(events.updateAvailable, (event) => setUpdate(event.payload));
    const offProgress = listen<UpdateProgress>(events.updateProgress, (event) => {
      const { downloaded, total, done } = event.payload;
      if (done) setProgress(100);
      else if (downloaded !== undefined && total) setProgress(Math.min(99, Math.round((downloaded / total) * 100)));
    });
    return () => {
      void offAvailable.then((fn) => fn());
      void offProgress.then((fn) => fn());
    };
  }, []);

  const check = useCallback(async () => {
    setChecking(true);
    setError(null);
    try {
      setUpdate(await ipc.checkForUpdate());
      setChecked(true);
    } catch (cause) {
      setError({ kind: 'check', message: String(cause) });
    } finally {
      setChecking(false);
    }
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    if (!update.can_install) {
      void openUrl(RELEASES_URL).catch(() => undefined);
      return;
    }
    setInstalling(true);
    setProgress(0);
    setError(null);
    try {
      // On success the app restarts and this never resolves.
      await ipc.installUpdate();
    } catch (cause) {
      setError({ kind: 'install', message: String(cause) });
      setInstalling(false);
      setProgress(null);
    }
  }, [update]);

  return { update, checking, checked, installing, progress, error, check, install };
}
