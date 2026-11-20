import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import type { MangaSession } from '../../lib/types';

/**
 * The manga reading session as Rust keeps it (`manga::session`): asked once,
 * then followed through `manga://session`. The clock is Rust's; this only
 * shows it. A clock that only runs is sent every `SESSION_RESYNC_SECS` (15 s),
 * so while it counts the seconds in between are added here, from when the
 * last session arrived.
 */
export function useMangaSession(): MangaSession | null {
  const [received, setReceived] = useState<{ session: MangaSession | null; at: number }>({ session: null, at: 0 });
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let alive = true;
    let heard = false;
    const off = listen<MangaSession | null>(events.mangaSession, (event) => {
      heard = true;
      setReceived({ session: event.payload, at: Date.now() });
    });
    void ipc.mangaSessionState()
      .then((value) => { if (alive && !heard) setReceived({ session: value, at: Date.now() }); })
      .catch(() => undefined);
    return () => {
      alive = false;
      void off.then((fn) => fn());
    };
  }, []);
  const counting = Boolean(received.session?.counting);
  useEffect(() => {
    if (!counting) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [counting, received]);
  return sessionAt(received.session, received.at, now);
}

/**
 * The session as it is at `now`, from the one that arrived at `at`: a clock
 * that counts has run the whole seconds in between.
 */
export function sessionAt(session: MangaSession | null, at: number, now: number): MangaSession | null {
  if (!session || !session.counting) return session;
  const ahead = Math.max(0, Math.floor((now - at) / 1000));
  return ahead > 0 ? { ...session, elapsed_secs: session.elapsed_secs + ahead } : session;
}
