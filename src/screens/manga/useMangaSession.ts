import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import type { MangaSession } from '../../lib/types';

/**
 * The manga reading session as Rust keeps it (`manga::session`): asked once,
 * then followed through `manga://session`. The clock is Rust's; this only
 * shows it.
 */
export function useMangaSession(): MangaSession | null {
  const [session, setSession] = useState<MangaSession | null>(null);
  useEffect(() => {
    let alive = true;
    let heard = false;
    const off = listen<MangaSession | null>(events.mangaSession, (event) => {
      heard = true;
      setSession(event.payload);
    });
    void ipc.mangaSessionState().then((value) => { if (alive && !heard) setSession(value); }).catch(() => undefined);
    return () => {
      alive = false;
      void off.then((fn) => fn());
    };
  }, []);
  return session;
}
