import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { BookmarkPlus, X } from 'lucide-react';
import type { TextLine } from '../../lib/types';
import { COLUMN_HEIGHT, FONT_FAMILY_STACKS } from './constants';
import { LINE_WINDOW_STEP, lineWindowStart, trimmedStart } from './lineWindow';

/** Without IntersectionObserver nothing could load older lines: render all. */
const CAN_WINDOW = typeof IntersectionObserver !== 'undefined';

interface LineListProps {
  lines: TextLine[];
  contentId: string;
  vertical: boolean;
  lineGap: number;
  lineSpacing: number;
  fontSize: number;
  fontFamily: string;
  saveLabel: string;
  deleteLabel: string;
  onPointer: (event: ReactMouseEvent<HTMLElement>, line: TextLine) => void;
  onLeave: () => void;
  onSave: (id: string) => void;
  onDelete: (id: string) => void;
}

/** The reader lines. Memoized with stable props, so opening a modal or the
 *  settings panel does not walk thousands of lines again. Only the last few
 *  hundred are rendered; scrolling near the first one shown adds older ones.
 *  `lines` stays whole: stats, undo, copy and rooms do not see the window. */
export const LineList = memo(function LineList({
  lines, contentId, vertical, lineGap, lineSpacing, fontSize, fontFamily,
  saveLabel, deleteLabel, onPointer, onLeave, onSave, onDelete,
}: LineListProps) {
  const listContainerRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  const lastScrollExtentRef = useRef<number | null>(null);

  // The window starts at a pinned line id; see `lineWindowStart`.
  const [pinnedId, setPinnedId] = useState<string | null>(null);
  const previousStartRef = useRef<number | null>(null);
  const previousLinesRef = useRef(lines);
  /** A line on screen, and where it was, before older lines were added. */
  const pendingAdjustRef = useRef<{ node: Element; left: number; top: number } | null>(null);
  const start = CAN_WINDOW ? lineWindowStart(lines, pinnedId, previousStartRef.current) : 0;
  const visibleLines = useMemo(() => (start > 0 ? lines.slice(start) : lines), [lines, start]);

  useLayoutEffect(() => {
    followLatestRef.current = true;
    lastScrollExtentRef.current = null;
  }, [contentId, vertical]);

  useLayoutEffect(() => {
    setPinnedId(null);
    previousStartRef.current = null;
    pendingAdjustRef.current = null;
  }, [contentId]);

  // Pin the window to its first line. While following the latest line, new
  // lines would grow it without end: past the slack it is trimmed back.
  useLayoutEffect(() => {
    let next = start;
    if (lines !== previousLinesRef.current && followLatestRef.current) {
      next = trimmedStart(lines.length, start);
    }
    previousLinesRef.current = lines;
    previousStartRef.current = next;
    const wanted = lines[next]?.id ?? null;
    if (CAN_WINDOW && wanted !== pinnedId) setPinnedId(wanted);
  }, [lines, start, pinnedId]);

  useLayoutEffect(() => {
    const el = listContainerRef.current;
    if (!el) return;
    const pending = pendingAdjustRef.current;
    pendingAdjustRef.current = null;
    const extent = vertical ? el.scrollWidth : el.scrollHeight;
    if (followLatestRef.current) {
      if (vertical) el.scrollLeft = Math.min(0, el.clientWidth - el.scrollWidth);
      else el.scrollTop = el.scrollHeight;
    } else if (pending?.node.isConnected) {
      // WebKit has no scroll anchoring: lines added above would push the text
      // being read. Put the line that was on screen back where it was; moving
      // the viewport by the same amount works for the RTL vertical list too.
      const rect = pending.node.getBoundingClientRect();
      if (vertical) el.scrollLeft += rect.left - pending.left;
      else el.scrollTop += rect.top - pending.top;
    }
    lastScrollExtentRef.current = extent;
  }, [visibleLines, vertical]);

  const loadOlder = useRef(() => {});
  loadOlder.current = () => {
    const el = listContainerRef.current;
    if (!el || start <= 0 || pendingAdjustRef.current) return;
    const view = el.getBoundingClientRect();
    const children = Array.from(el.children);
    const node = children.find((child) => {
      const rect = child.getBoundingClientRect();
      return vertical ? rect.left < view.right : rect.bottom > view.top;
    }) ?? children[0];
    if (!node) return;
    const rect = node.getBoundingClientRect();
    pendingAdjustRef.current = { node, left: rect.left, top: rect.top };
    setPinnedId(lines[Math.max(0, start - LINE_WINDOW_STEP)].id);
  };

  // The first line shown is the sentinel: once it comes near, add older ones.
  useEffect(() => {
    const first = listContainerRef.current?.firstElementChild;
    if (!CAN_WINDOW || start <= 0 || !first) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) loadOlder.current();
      },
      { root: listContainerRef.current, rootMargin: '600px' },
    );
    observer.observe(first);
    return () => observer.disconnect();
  }, [visibleLines, start, vertical]);

  const listStyles = useMemo<CSSProperties>(() => (vertical
    ? { columnGap: lineGap, rowGap: lineGap, direction: 'rtl' }
    : { rowGap: lineGap }), [vertical, lineGap]);

  // Memoized, like the handlers, so `LineItem` can skip the lines that did not
  // change.
  const lineTextStyle = useMemo<CSSProperties>(() => {
    const base: CSSProperties = {
      lineHeight: lineSpacing,
      fontSize,
      fontFamily:
        FONT_FAMILY_STACKS[fontFamily] ||
        `"${fontFamily}", "Noto Sans JP Local", "Noto Sans JP", sans-serif`,
    };
    return vertical
      ? { ...base, writingMode: 'vertical-rl', textOrientation: 'mixed', flexShrink: 0, height: '100%', direction: 'ltr' }
      : { ...base, display: 'block' };
  }, [lineSpacing, fontSize, fontFamily, vertical]);

  const listContainerClasses = vertical
    ? 'pt-20 px-4 flex flex-row overflow-x-auto overflow-y-hidden h-screen items-start pb-6'
    : 'pt-20 px-4 flex flex-col overflow-y-auto h-screen pb-6';

  return (
    <div
      ref={listContainerRef}
      className={`${listContainerClasses} th-line-list`}
      style={listStyles}
      onScroll={(event) => {
        const el = event.currentTarget;
        const extent = vertical ? el.scrollWidth : el.scrollHeight;
        if (lastScrollExtentRef.current !== extent) {
          lastScrollExtentRef.current = extent;
          return;
        }
        followLatestRef.current = vertical
          ? Math.abs(el.scrollLeft - Math.min(0, el.clientWidth - el.scrollWidth)) < 48
          : el.scrollHeight - el.clientHeight - el.scrollTop < 48;
      }}
    >
      {visibleLines.map((line) => (
        <LineItem
          key={line.id}
          line={line}
          vertical={vertical}
          textStyle={lineTextStyle}
          saveLabel={saveLabel}
          deleteLabel={deleteLabel}
          onPointer={onPointer}
          onLeave={onLeave}
          onSave={onSave}
          onDelete={onDelete}
        />
      ))}
    </div>
  );
});

interface LineItemProps {
  line: TextLine;
  vertical: boolean;
  textStyle: CSSProperties;
  saveLabel: string;
  deleteLabel: string;
  onPointer: (event: ReactMouseEvent<HTMLElement>, line: TextLine) => void;
  onLeave: () => void;
  onSave: (id: string) => void;
  onDelete: (id: string) => void;
}

/** One reader line. Memoized: a long session holds thousands of them, and the
 *  screen around them re-renders with the session timer every second. */
const LineItem = memo(function LineItem({
  line, vertical, textStyle, saveLabel, deleteLabel, onPointer, onLeave, onSave, onDelete,
}: LineItemProps) {
  if (vertical) {
    return (
      <div
        className="group relative flex flex-col items-end justify-start px-2 pt-6 text-base-content shrink-0 th-line-item"
        style={{ minHeight: COLUMN_HEIGHT, height: COLUMN_HEIGHT }}
      >
        <span
          lang="ja"
          className="block text-lg whitespace-pre-wrap wrap-break-word th-line-text selectable cursor-text"
          style={textStyle}
          onMouseMove={(event) => onPointer(event, line)}
          onMouseLeave={onLeave}
        >
          {line.text}
          <button
            type="button"
            className="inline-flex justify-center items-center text-primary text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
            aria-label={saveLabel}
            onClick={() => onSave(line.id)}
            style={{ writingMode: 'horizontal-tb', display: 'inline-flex', marginTop: '0.5rem', direction: 'ltr' }}
          >
            <BookmarkPlus className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            className="inline-flex justify-center items-center text-error text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
            aria-label={deleteLabel}
            onClick={() => onDelete(line.id)}
            style={{
              writingMode: 'horizontal-tb',
              display: 'inline-flex',
              marginTop: '0.5rem',
              direction: 'ltr',
            }}
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </span>
      </div>
    );
  }

  return (
    <div className="group relative px-2 py-2 text-base-content shrink-0 th-line-item">
      <p
        className="whitespace-pre-wrap wrap-break-word th-line-text selectable cursor-text"
        style={textStyle}
        onMouseMove={(event) => onPointer(event, line)}
        onMouseLeave={onLeave}
      >
        {line.text}
        <button type="button" className="ml-2 align-baseline text-primary text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity" aria-label={saveLabel} onClick={() => onSave(line.id)}>
          <BookmarkPlus className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className="ml-2 align-baseline text-error text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
          aria-label={deleteLabel}
          onClick={() => onDelete(line.id)}
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </p>
    </div>
  );
});
