import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../lib/ipc';
import type { ReadingOverlayState } from '../lib/types';

export function useReadingOverlay() {
  const [view, setView] = useState<ReadingOverlayState | null>(null);
  useEffect(() => {
    let disposed = false;
    let receivedEvent = false;
    const unlisten = listen<ReadingOverlayState>(events.readingOverlay, (event) => {
      receivedEvent = true;
      if (!disposed) setView(event.payload);
    });
    void unlisten.then(async () => {
      const state = await ipc.readingOverlayState();
      if (!disposed && !receivedEvent) setView(state);
    });
    return () => { disposed = true; void unlisten.then((stop) => stop()); };
  }, []);
  return view;
}
