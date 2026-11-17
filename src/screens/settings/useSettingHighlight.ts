import { useCallback, useEffect, useState, type RefObject } from 'react';
import { normalizeSearch } from '../../lib/search';

const HIGHLIGHT_MS = 2400;

/** The row that shows `prefix`: a Toggle is a <label>, a Field a <fieldset>. */
function findRow(root: HTMLElement, prefix: string): HTMLElement | null {
  const wanted = normalizeSearch(prefix);
  if (!wanted) return null;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = normalizeSearch(node.textContent?.trim() ?? '');
    if (!text || !text.startsWith(wanted)) continue;
    const element = node.parentElement;
    if (!element || element.closest('h2') === root.firstElementChild) continue;
    return element.closest<HTMLElement>('label, fieldset') ?? element;
  }
  return null;
}

/**
 * Scrolls to a row of the open section and rings it for a moment. Returns
 * `request(labelPrefix)`; the row is looked for after the section renders,
 * and again briefly in case it waits for data (dictionaries, Anki fields).
 */
export function useSettingHighlight(panel: RefObject<HTMLElement | null>) {
  const [pending, setPending] = useState<{ text: string; nonce: number } | null>(null);

  useEffect(() => {
    if (!pending) return;
    let tries = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let clear: ReturnType<typeof setTimeout> | undefined;
    let row: HTMLElement | null = null;
    const attempt = () => {
      const root = panel.current;
      row = root ? findRow(root, pending.text) : null;
      if (!row) {
        if (tries++ < 10) retry = setTimeout(attempt, 100);
        return;
      }
      const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      row.scrollIntoView({ block: 'center', behavior: reduced ? 'auto' : 'smooth' });
      row.querySelector<HTMLElement>('input, select, textarea, button')?.focus({ preventScroll: true });
      row.classList.remove('setting-highlight');
      // Restart the animation when the same row is picked twice.
      void row.offsetWidth;
      row.classList.add('setting-highlight');
      const target = row;
      clear = setTimeout(() => target.classList.remove('setting-highlight'), HIGHLIGHT_MS);
    };
    attempt();
    return () => {
      clearTimeout(retry);
      clearTimeout(clear);
      row?.classList.remove('setting-highlight');
    };
  }, [pending, panel]);

  return useCallback((text: string) => setPending({ text, nonce: Date.now() }), []);
}
