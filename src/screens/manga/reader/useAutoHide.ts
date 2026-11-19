import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * The reader's controls: up when the mouse moves, gone a moment after it stops,
 * and kept up while the pointer is on them or a panel is open (`pin`). The
 * keyboard never brings them up: turning pages should not flash the bars.
 *
 * `toggleLock` (a double click on the page) keeps them hidden however the
 * mouse moves, until it is called again: reading with the mouse in hand
 * should not bring the bars back at every nudge.
 */
export function useAutoHide(delayMs = 2500) {
  const [visible, setVisible] = useState(true);
  const [locked, setLocked] = useState(false);
  const lockedRef = useRef(false);
  /** The mouse has not moved for a while: the cursor can go too. */
  const [idle, setIdle] = useState(false);
  const timer = useRef<number | null>(null);
  const pins = useRef(new Set<string>());

  const schedule = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setIdle(true);
      if (pins.current.size === 0) setVisible(false);
    }, delayMs);
  }, [delayMs]);

  const show = useCallback(() => {
    // The cursor comes back with any move, even with the controls locked away.
    setIdle(false);
    if (!lockedRef.current) setVisible(true);
    schedule();
  }, [schedule]);

  const hide = useCallback(() => {
    if (pins.current.size === 0) setVisible(false);
  }, []);

  const pin = useCallback((reason: string, pinned: boolean) => {
    if (pinned) pins.current.add(reason);
    else pins.current.delete(reason);
    if (pinned) setVisible(true);
    else schedule();
  }, [schedule]);

  const toggleLock = useCallback(() => {
    const next = !lockedRef.current;
    lockedRef.current = next;
    setLocked(next);
    if (next) {
      // The bars are gone, so the pointer is no longer on them.
      pins.current.delete('top');
      pins.current.delete('bottom');
      setVisible(false);
    } else {
      setVisible(true);
      schedule();
    }
  }, [schedule]);

  useEffect(() => {
    schedule();
    let last = { x: -1, y: -1 };
    const onMove = (event: MouseEvent) => {
      // Some systems send a move with no movement after a key press.
      if (event.clientX === last.x && event.clientY === last.y) return;
      last = { x: event.clientX, y: event.clientY };
      show();
    };
    window.addEventListener('mousemove', onMove);
    return () => {
      window.removeEventListener('mousemove', onMove);
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [schedule, show]);

  return {
    visible: visible && !locked,
    /** Hide the cursor: only once the mouse stops, never because of the lock. */
    cursorHidden: idle && !(visible && !locked),
    locked,
    show,
    hide,
    pin,
    toggleLock,
  };
}
