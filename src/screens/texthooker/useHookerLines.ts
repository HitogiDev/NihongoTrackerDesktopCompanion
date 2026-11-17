import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { events, ipc } from '../../lib/ipc';
import type { HookerTotals, TextLine } from '../../lib/types';

/** Lines kept in memory for the open work. */
export const LINE_HISTORY_LIMIT = 5000;

interface Options {
  contentId: string;
  /** A detected visual novel keeps its own clock: no stored timer to restore. */
  hasAutoSession: boolean;
  /** The stored manual timer, after each reload. */
  onTimerLoaded: (seconds: number) => void;
  /** The screen switched to another work. */
  onSwitch: () => void;
  /** A line for this work arrived. */
  onLine: () => void;
}

/** The lines and totals of the open work, kept in step with Rust. */
export function useHookerLines({ contentId, hasAutoSession, onTimerLoaded, onSwitch, onLine }: Options) {
  const contentIdRef = useRef(contentId);
  contentIdRef.current = contentId;
  const reloadRequestRef = useRef(0);
  const displayedContentIdRef = useRef(contentId);
  const callbacks = useRef({ onTimerLoaded, onSwitch, onLine });
  callbacks.current = { onTimerLoaded, onSwitch, onLine };

  const [lines, setLines] = useState<TextLine[]>([]);
  const [totals, setTotals] = useState<HookerTotals | null>(null);
  const [undoStack, setUndoStack] = useState<TextLine[][]>([]);

  const reload = useCallback(async () => {
    if (contentIdRef.current !== contentId) return;
    const requestId = ++reloadRequestRef.current;
    const [nextLines, nextTotals] = await Promise.all([
      ipc.hookerLines(contentId, LINE_HISTORY_LIMIT),
      ipc.hookerTotals(contentId),
    ]);
    if (
      requestId !== reloadRequestRef.current ||
      contentIdRef.current !== contentId
    ) return;
    setLines(nextLines);
    setTotals(nextTotals);
    if (!hasAutoSession) callbacks.current.onTimerLoaded(nextTotals.timerSeconds);
  }, [contentId, hasAutoSession]);

  useEffect(() => {
    if (displayedContentIdRef.current !== contentId) {
      displayedContentIdRef.current = contentId;
      setLines([]);
      setTotals(null);
      callbacks.current.onSwitch();
      setUndoStack([]);
    }
    void reload();
  }, [contentId, reload]);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<{ line: TextLine; totals: { lines: number; chars: number } }>(
      events.hookerLine,
      (event) => {
        if (disposed) return;
        if (event.payload.line.content_id !== contentId) return;
        callbacks.current.onLine();
        setLines((prev) => prev.some((line) => line.id === event.payload.line.id)
          ? prev : [...prev, event.payload.line]);
        setTotals((prev) =>
          prev
            ? {
                ...prev,
                lines: event.payload.totals.lines,
                chars: event.payload.totals.chars,
                sessionChars: Math.max(0, event.payload.totals.chars - prev.baselineChars),
                sessionLines: Math.max(0, event.payload.totals.lines - prev.baselineLines),
              }
            : prev
        );
      }
    );
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [contentId]);

  const handleDelete = useCallback(
    async (id: string) => {
      const removed = lines.find((line) => line.id === id);
      if (!removed) return;
      setUndoStack((prev) => [...prev, [removed]]);
      setLines((prev) => prev.filter((line) => line.id !== id));
      await ipc.hookerDeleteLine(id);
      setTotals(await ipc.hookerTotals(contentId));
    },
    [lines, contentId]
  );

  const handleDeleteLast = useCallback(() => {
    const last = lines[lines.length - 1];
    if (last) void handleDelete(last.id);
  }, [lines, handleDelete]);

  const handleUndo = useCallback(async () => {
    const batch = undoStack[undoStack.length - 1];
    if (!batch) return;
    setUndoStack((prev) => prev.slice(0, -1));
    for (const line of batch) {
      await ipc.hookerRestoreLine(line);
    }
    await reload();
  }, [undoStack, reload]);

  const pushUndo = useCallback((batch: TextLine[]) => {
    setUndoStack((prev) => [...prev, batch]);
  }, []);

  const handleCopyAll = useCallback(async () => {
    const combined = lines.map((line) => line.text).join('\n');
    if (!combined) return;
    await navigator.clipboard.writeText(combined).catch(() => undefined);
  }, [lines]);

  return {
    contentIdRef,
    lines,
    totals,
    canUndo: undoStack.length > 0,
    reload,
    handleDelete,
    handleDeleteLast,
    handleUndo,
    pushUndo,
    handleCopyAll,
  };
}
