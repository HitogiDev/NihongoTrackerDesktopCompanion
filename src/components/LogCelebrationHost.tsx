import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import LogCelebration from './LogCelebration';
import { celebrateLog, useCelebrationStore } from '../store/useCelebrationStore';
import { events } from '../lib/ipc';
import type { LogCelebration as Celebration } from '../lib/types';

interface CelebrationEvent {
  celebration: Celebration | null;
  newAchievements: unknown[] | null;
}

/**
 * Mounted once, next to the router. The celebration payload comes from the
 * `POST /api/logs` response, which the Rust outbox forwards — so a log that
 * was queued offline still celebrates when it finally lands.
 */
export default function LogCelebrationHost() {
  const pending = useCelebrationStore((s) => s.pending);
  const achievements = useCelebrationStore((s) => s.achievements);
  const close = useCelebrationStore((s) => s.close);

  useEffect(() => {
    const unlisten = listen<CelebrationEvent>(events.celebration, (event) => {
      celebrateLog(
        event.payload.celebration ?? undefined,
        event.payload.newAchievements?.length ?? 0
      );
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  if (!pending) return null;

  return <LogCelebration celebration={pending} achievements={achievements} onClose={close} />;
}
