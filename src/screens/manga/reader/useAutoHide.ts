import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * The reader's controls: up when the mouse moves, gone a moment after it stops,
 * and kept up while the pointer is on them or a panel is open (`pin`). The
 * keyboard never brings them up: turning pages should not flash the bars.
 */
export function useAutoHide(delayMs = 2500) {
  const [visible, setVisible] = useState(true);
  const timer = useRef<number | null>(null);
  const pins = useRef(new Set<string>());

  const schedule = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (pins.current.size === 0) setVisible(false);
    }, delayMs);
  }, [delayMs]);

  const show = useCallback(() => {
    setVisible(true);
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

  return { visible, show, hide, pin, toggle: () => (visible ? hide() : show()) };
}
