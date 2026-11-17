import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import { useAppStore } from '../../store/useAppStore';

/**
 * Where the session time comes from: the watcher's clock for a detected visual
 * novel, the manual timer otherwise. The seconds are published outside React
 * state so only the components that print them re-render each second.
 */
export interface SessionClockSource {
  contentId: string;
  subscribe: (listener: () => void) => () => void;
  /** The manual timer's seconds. */
  getManualSeconds: () => number;
  /** The seconds the screen shows right now, for actions (log, edit). */
  current: () => number;
}

export interface ManualTimer {
  isTimerActive: boolean;
  setIsTimerActive: Dispatch<SetStateAction<boolean>>;
  secondsRef: MutableRefObject<number>;
  lastActivityRef: MutableRefObject<number>;
  anchorTimer: (value: number) => void;
  /** Sets the clock to `value`, anchored to now. */
  setSeconds: (value: number) => void;
  clock: SessionClockSource;
}

function detectedElapsed(contentId: string): number | null {
  const vn = useAppStore.getState().tracker?.vn;
  return vn && vn.content_id === contentId ? vn.elapsed_secs : null;
}

export function useManualTimer({
  contentId,
  hasAutoSession,
  autoPauseTimeout,
}: {
  contentId: string;
  hasAutoSession: boolean;
  autoPauseTimeout: number;
}): ManualTimer {
  const [isTimerActive, setIsTimerActive] = useState(false);
  const anchorRef = useRef({ at: Date.now(), seconds: 0 });
  const secondsRef = useRef(0);
  const lastActivityRef = useRef(Date.now());
  const listenersRef = useRef(new Set<() => void>());

  const notify = useCallback(() => {
    listenersRef.current.forEach((listener) => listener());
  }, []);

  const anchorTimer = useCallback((value: number) => {
    anchorRef.current = { at: Date.now(), seconds: value };
  }, []);

  const setSeconds = useCallback((value: number) => {
    secondsRef.current = value;
    anchorTimer(value);
    notify();
  }, [anchorTimer, notify]);

  // Rust starts the clock when a line arrives and autostart is on.
  useEffect(() => {
    const unlisten = listen<{ active: boolean }>(events.hookerTimer, (event) => {
      setIsTimerActive(event.payload.active);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    void ipc.hookerSetTimerActive(isTimerActive);
  }, [isTimerActive]);

  // Wall-clock anchored, exactly like the web client: a throttled render loop
  // must never lose time, and the auto-pause deadline caps how much of a gap
  // can be banked.
  useEffect(() => {
    if (hasAutoSession || !isTimerActive) return;
    anchorTimer(secondsRef.current);

    const tick = () => {
      const now = Date.now();
      const pauseAt =
        autoPauseTimeout > 0 ? lastActivityRef.current + autoPauseTimeout * 1000 : Infinity;
      const countedUntil = Math.min(now, pauseAt);
      const { at, seconds: banked } = anchorRef.current;
      const next = banked + Math.max(0, Math.floor((countedUntil - at) / 1000));

      if (next !== secondsRef.current) {
        secondsRef.current = next;
        notify();
      }
      if (now >= pauseAt) {
        anchorTimer(next);
        setIsTimerActive(false);
      }
    };

    const interval = setInterval(tick, 1000);
    document.addEventListener('visibilitychange', tick);
    return () => {
      clearInterval(interval);
      document.removeEventListener('visibilitychange', tick);
    };
  }, [isTimerActive, autoPauseTimeout, hasAutoSession, anchorTimer, notify]);

  // Persist the manual clock so a restart doesn't lose the sitting.
  useEffect(() => {
    if (hasAutoSession) return;
    const id = setInterval(() => {
      void ipc.hookerSetTimer(contentId, secondsRef.current);
    }, 15000);
    return () => {
      clearInterval(id);
      void ipc.hookerSetTimer(contentId, secondsRef.current);
    };
  }, [contentId, hasAutoSession]);

  const clock = useMemo<SessionClockSource>(() => ({
    contentId,
    subscribe: (listener) => {
      listenersRef.current.add(listener);
      return () => listenersRef.current.delete(listener);
    },
    getManualSeconds: () => secondsRef.current,
    current: () => detectedElapsed(contentId) ?? secondsRef.current,
  }), [contentId]);

  return {
    isTimerActive,
    setIsTimerActive,
    secondsRef,
    lastActivityRef,
    anchorTimer,
    setSeconds,
    clock,
  };
}

/** The session seconds, re-rendering only the component that calls it. */
export function useSessionSeconds(clock: SessionClockSource): number {
  const detected = useAppStore((s) =>
    s.tracker?.vn && s.tracker.vn.content_id === clock.contentId ? s.tracker.vn.elapsed_secs : null);
  const manual = useSyncExternalStore(clock.subscribe, clock.getManualSeconds);
  return detected ?? manual;
}
