import { useEffect, useState } from 'react';

/** How long daisyUI's modal takes to fade out. */
const CLOSE_MS = 300;

/**
 * Mount a modal's content only while it is open, without losing daisyUI's
 * transitions: the content goes in one frame before `modal-open` so the box
 * animates in, and stays until the fade-out ends.
 */
export function useModalPresence(open: boolean) {
  const [mounted, setMounted] = useState(open);
  const [shown, setShown] = useState(open);

  useEffect(() => {
    if (open) {
      setMounted(true);
      let inner = 0;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => setShown(true));
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    }
    setShown(false);
    const id = setTimeout(() => setMounted(false), CLOSE_MS);
    return () => clearTimeout(id);
  }, [open]);

  return { mounted: mounted || open, shown: shown && open };
}
