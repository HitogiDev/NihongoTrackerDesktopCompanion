import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';
import type { LookupRequest } from '@nihongotracker/yomitan-content';
import { ipc } from '../lib/ipc';
import { textHitAtPoint } from '../lib/textHit';
import type { DictionaryEntry, TextLine } from '../lib/types';

/**
 * Shift/Alt hover lookups over text, and the popup they open. Shared by the
 * texthooker (its lines) and the manga reader (its text blocks).
 */
export function useDictionaryHover(dictionaryPopupDelay: number) {
  const [dictionaryRequest, setDictionaryRequest] = useState<LookupRequest | null>(null);
  const [dictionaryAnchor, setDictionaryAnchor] = useState<DOMRect | null>(null);
  const [dictionaryVertical, setDictionaryVertical] = useState(false);
  const [dictionaryEntries, setDictionaryEntries] = useState<DictionaryEntry[] | undefined>();
  const [dictionaryLoading, setDictionaryLoading] = useState(false);
  const dictionaryHoverTimerRef = useRef<number | null>(null);
  const dictionaryHoverKeyRef = useRef<string | null>(null);
  const dictionaryPointerRef = useRef<{ element: HTMLElement; text: string; x: number; y: number; context?: { lineId: string; contentId: string } } | null>(null);

  const lookupDictionary = useCallback(async (request: LookupRequest) => {
    setDictionaryLoading(true);
    try {
      const response = await ipc.dictionaryLookup(request.query, request.offset ?? 0);
      setDictionaryEntries(response.entries);
    } catch {
      setDictionaryEntries([]);
    } finally {
      setDictionaryLoading(false);
    }
  }, []);

  const [dictionaryContext, setDictionaryContext] = useState<{ lineId: string; contentId: string }>();
  const scheduleDictionaryLookup = useCallback((element: HTMLElement, text: string, x: number, y: number, context?: { lineId: string; contentId: string }) => {
    const hit = textHitAtPoint(element, x, y);
    if (!hit) {
      if (dictionaryHoverTimerRef.current !== null) window.clearTimeout(dictionaryHoverTimerRef.current);
      dictionaryHoverTimerRef.current = null;
      dictionaryHoverKeyRef.current = null;
      return;
    }
    const { offset, rect: anchorRect } = hit;

    const key = `${text}:${offset}`;
    if (dictionaryHoverKeyRef.current === key) return;
    dictionaryHoverKeyRef.current = key;
    if (dictionaryHoverTimerRef.current !== null) {
      window.clearTimeout(dictionaryHoverTimerRef.current);
    }
    dictionaryHoverTimerRef.current = window.setTimeout(() => {
      setDictionaryContext(context);
      setDictionaryAnchor(anchorRect);
      setDictionaryVertical(hit.vertical);
      setDictionaryEntries(undefined);
      setDictionaryRequest({ query: text, offset });
      dictionaryHoverTimerRef.current = null;
    }, dictionaryPopupDelay);
  }, [dictionaryPopupDelay]);

  /** Any element whose text content is `text`; `context` lets the popup mine it. */
  const openDictionaryAt = useCallback((event: ReactMouseEvent<HTMLElement>, text: string, context?: { lineId: string; contentId: string }) => {
    const element = event.currentTarget;
    dictionaryPointerRef.current = { element, text, x: event.clientX, y: event.clientY, context };
    if (!event.shiftKey && !event.altKey) {
      if (dictionaryHoverTimerRef.current !== null) {
        window.clearTimeout(dictionaryHoverTimerRef.current);
        dictionaryHoverTimerRef.current = null;
      }
      dictionaryHoverKeyRef.current = null;
      return;
    }
    scheduleDictionaryLookup(element, text, event.clientX, event.clientY, context);
  }, [scheduleDictionaryLookup]);

  const openDictionaryAtPointer = useCallback(
    (event: ReactMouseEvent<HTMLElement>, line: TextLine) =>
      openDictionaryAt(event, line.text, { lineId: line.id, contentId: line.content_id }),
    [openDictionaryAt],
  );

  const cancelDictionaryHover = useCallback(() => {
    if (dictionaryHoverTimerRef.current !== null) {
      window.clearTimeout(dictionaryHoverTimerRef.current);
      dictionaryHoverTimerRef.current = null;
    }
    dictionaryHoverKeyRef.current = null;
    dictionaryPointerRef.current = null;
  }, []);

  useEffect(() => {
    const lookupOnModifierPress = (event: KeyboardEvent) => {
      if ((event.key !== 'Shift' && event.key !== 'Alt') || event.repeat) return;
      const activeElement = document.activeElement;
      if (
        activeElement instanceof HTMLElement &&
        (activeElement.isContentEditable || activeElement.matches('input, textarea, select'))
      ) {
        return;
      }
      const pointer = dictionaryPointerRef.current;
      if (pointer) scheduleDictionaryLookup(pointer.element, pointer.text, pointer.x, pointer.y, pointer.context);
    };

    document.addEventListener('keydown', lookupOnModifierPress);
    return () => document.removeEventListener('keydown', lookupOnModifierPress);
  }, [scheduleDictionaryLookup]);

  const closeDictionary = useCallback(() => {
    setDictionaryRequest(null);
    setDictionaryAnchor(null);
  }, []);

  return {
    dictionaryRequest,
    dictionaryContext,
    dictionaryEntries,
    dictionaryLoading,
    dictionaryAnchor,
    dictionaryVertical,
    lookupDictionary,
    openDictionaryAt,
    openDictionaryAtPointer,
    cancelDictionaryHover,
    closeDictionary,
  };
}
