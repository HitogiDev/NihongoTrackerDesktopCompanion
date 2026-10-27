import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { emit, listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, BookOpen, Plus, Volume2, X } from 'lucide-react';
import { StructuredContent, type Glossary, type LookupRequest } from '@nihongotracker/yomitan-content';
import { Button } from './ui/Button';
import type { DictionaryEntry } from '../lib/types';
import { ipc } from '../lib/ipc';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Grip } from 'lucide-react';
import { miningError as formatMiningError } from '../lib/mining';

const CARD_THEMES = [
  { card: 'border-error/50 bg-error/10', badge: 'badge-error' },
  { card: 'border-warning/50 bg-warning/10', badge: 'badge-warning' },
  { card: 'border-info/50 bg-info/10', badge: 'badge-info' },
  { card: 'border-primary/50 bg-primary/10', badge: 'badge-primary' },
  { card: 'border-success/50 bg-success/10', badge: 'badge-success' },
] as const;

const POPUP_SIZE_KEY = 'nihongo-tracker.dictionary-popup-size';
const DEFAULT_POPUP_SIZE = { width: 560, height: 560 };
const POPUP_LEAVE_CLOSE_DELAY_MS = 500;

function readPopupSize() {
  try {
    const stored = localStorage.getItem(POPUP_SIZE_KEY);
    if (!stored) return DEFAULT_POPUP_SIZE;
    const parsed: unknown = JSON.parse(stored);
    if (!parsed || typeof parsed !== 'object') return DEFAULT_POPUP_SIZE;
    const value = parsed as { width?: unknown; height?: unknown };
    if (typeof value.width !== 'number' || typeof value.height !== 'number') return DEFAULT_POPUP_SIZE;
    return {
      width: Math.max(320, Math.min(1600, value.width)),
      height: Math.max(280, Math.min(1200, value.height)),
    };
  } catch {
    return DEFAULT_POPUP_SIZE;
  }
}

export function DictionaryPopup({
  initial,
  entries,
  loading,
  anchor,
  vertical = false,
  theme,
  onLookup,
  onClose,
  windowMode = false,
  miningContext,
  backgroundOpacity,
  closeDelayMs = POPUP_LEAVE_CLOSE_DELAY_MS,
}: {
  initial: LookupRequest;
  entries: DictionaryEntry[] | undefined;
  loading: boolean;
  anchor: DOMRect | null;
  vertical?: boolean;
  theme?: string;
  onLookup: (request: LookupRequest) => void;
  onClose: () => void;
  windowMode?: boolean;
  miningContext?: { lineId: string; contentId: string };
  backgroundOpacity?: number;
  closeDelayMs?: number;
}) {
  const { t } = useTranslation('dictionary');
  const [history, setHistory] = useState<LookupRequest[]>([initial]);
  const [size, setSize] = useState(readPopupSize);
  const [position, setPosition] = useState<{
    left: number | null;
    right: number | null;
    top: number;
    width: number;
    height: number;
  } | null>(null);
  const [mining, setMining] = useState<string | null>(null);
  const [miningError, setMiningError] = useState<string | null>(null);
  const popupRef = useRef<HTMLElement>(null);
  const resizingRef = useRef(false);
  const hasBeenEnteredRef = useRef(false);
  const onCloseRef = useRef(onClose);
  const leaveCloseTimerRef = useRef<number | null>(null);
  const current = history[history.length - 1] ?? initial;
  const headEntry = entries?.[0];
  const headExpression =
    headEntry?.expression || (current.offset === undefined && entries !== undefined ? current.query : '');
  const headReading = headEntry?.reading && headEntry.reading !== headExpression ? headEntry.reading : '';

  useEffect(() => {
    setHistory([initial]);
    hasBeenEnteredRef.current = false;
  }, [initial]);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (!windowMode) return;
    const listener = listen('reading-overlay-text-enter', () => {
      if (leaveCloseTimerRef.current !== null) {
        window.clearTimeout(leaveCloseTimerRef.current);
        leaveCloseTimerRef.current = null;
      }
    });
    return () => { void listener.then((stop) => stop()); };
  }, [windowMode]);

  useEffect(() => {
    const closeOnOutsidePointerDown = (event: PointerEvent) => {
      const popup = popupRef.current;
      if (popup && event.target instanceof Node && !popup.contains(event.target)) {
        if (leaveCloseTimerRef.current !== null) {
          window.clearTimeout(leaveCloseTimerRef.current);
          leaveCloseTimerRef.current = null;
        }
        onCloseRef.current();
      }
    };

    document.addEventListener('pointerdown', closeOnOutsidePointerDown);
    return () => {
      document.removeEventListener('pointerdown', closeOnOutsidePointerDown);
      if (leaveCloseTimerRef.current !== null) window.clearTimeout(leaveCloseTimerRef.current);
    };
  }, []);

  useEffect(() => {
    onLookup(current);
  }, [current, onLookup]);

  useLayoutEffect(() => {
    const reposition = () => {
      if (windowMode) {
        setPosition({ left: 0, right: null, top: 0, width: window.innerWidth, height: window.innerHeight });
        return;
      }
      const target = anchor ?? new DOMRect(window.innerWidth / 2, window.innerHeight / 2, 0, 0);
      const margin = 12;
      const gap = 12;
      const viewportWidth = document.documentElement.clientWidth;
      const viewportHeight = document.documentElement.clientHeight;
      if (!vertical) {
        const below = Math.max(0, viewportHeight - target.bottom - gap - margin);
        const above = Math.max(0, target.top - gap - margin);
        const openBelow = below >= size.height || (above < size.height && below >= above);
        const width = Math.min(size.width, Math.max(0, viewportWidth - margin * 2));
        const height = Math.min(size.height, openBelow ? below : above);
        const left = Math.max(margin, Math.min(target.left, viewportWidth - width - margin));
        const top = openBelow ? target.bottom + gap : target.top - gap - height;
        setPosition({ left, right: null, top, width, height });
        return;
      }
      const rightSpace = Math.max(0, viewportWidth - target.right - gap - margin);
      const leftSpace = Math.max(0, target.left - gap - margin);
      const openRight = rightSpace >= leftSpace;
      const sideSpace = openRight ? rightSpace : leftSpace;
      const maxWidth = Math.max(0, Math.min(viewportWidth - margin * 2, sideSpace));
      const width = Math.min(size.width, maxWidth);
      const height = Math.min(size.height, Math.max(0, viewportHeight - margin * 2));
      const left = openRight ? target.right + gap : null;
      const right = openRight ? null : viewportWidth - target.left + gap;
      const top = Math.max(margin, Math.min(target.top + (target.height - height) / 2, viewportHeight - height - margin));
      setPosition({ left, right, top, width, height });
    };

    reposition();
    window.addEventListener('resize', reposition);
    window.visualViewport?.addEventListener('resize', reposition);
    return () => {
      window.removeEventListener('resize', reposition);
      window.visualViewport?.removeEventListener('resize', reposition);
    };
  }, [anchor, size, windowMode, vertical]);

  useEffect(() => {
    const finishResize = () => {
      if (!resizingRef.current) return;
      resizingRef.current = false;
      requestAnimationFrame(() => {
        const rect = popupRef.current?.getBoundingClientRect();
        if (!rect) return;
        const nextSize = { width: Math.round(rect.width), height: Math.round(rect.height) };
        setSize(nextSize);
        try {
          localStorage.setItem(POPUP_SIZE_KEY, JSON.stringify(nextSize));
        } catch {
          // The popup still works when browser storage is unavailable.
        }
      });
    };

    window.addEventListener('pointerup', finishResize);
    window.addEventListener('pointercancel', finishResize);
    return () => {
      window.removeEventListener('pointerup', finishResize);
      window.removeEventListener('pointercancel', finishResize);
    };
  }, []);

  function push(request: LookupRequest) {
    setHistory((previous) => [...previous, request]);
  }

  function meaningText(value: unknown): string {
    if (typeof value === 'string') return value;
    if (Array.isArray(value)) return value.map(meaningText).filter(Boolean).join('; ');
    if (value && typeof value === 'object') {
      const item = value as Record<string, unknown>;
      for (const key of ['content', 'text', 'html', 'glossary']) {
        if (key in item) return meaningText(item[key]);
      }
    }
    return '';
  }

  function scheduleLeaveClose() {
    if (leaveCloseTimerRef.current !== null) window.clearTimeout(leaveCloseTimerRef.current);
    leaveCloseTimerRef.current = window.setTimeout(() => {
      if (resizingRef.current) {
        scheduleLeaveClose();
        return;
      }
      leaveCloseTimerRef.current = null;
      onCloseRef.current();
    }, closeDelayMs);
  }

  return (
    <aside
      ref={popupRef}
      role="dialog"
      aria-label={t('title')}
      data-theme={theme}
      onMouseEnter={() => {
        hasBeenEnteredRef.current = true;
        if (windowMode) void emit('reading-overlay-popup-enter');
        if (leaveCloseTimerRef.current !== null) {
          window.clearTimeout(leaveCloseTimerRef.current);
          leaveCloseTimerRef.current = null;
        }
      }}
      onMouseLeave={() => {
        if (!hasBeenEnteredRef.current) return;
        scheduleLeaveClose();
      }}
      onPointerDown={(event) => {
        const rect = event.currentTarget.getBoundingClientRect();
        resizingRef.current = event.clientX >= rect.right - 24 && event.clientY >= rect.bottom - 24;
      }}
      className="fixed z-[100] flex flex-col overflow-hidden rounded-box border border-base-content/15 border-l-2 border-l-info bg-base-100/80 text-base-content shadow-lg backdrop-blur-md [resize:both]"
      style={{
        backgroundColor: backgroundOpacity === undefined ? undefined : `color-mix(in srgb, var(--color-base-100) ${Math.max(0, Math.min(100, backgroundOpacity))}%, transparent)`,
        left: position?.left ?? 'auto',
        right: position?.right ?? 'auto',
        top: position?.top ?? 0,
        width: position?.width ?? 0,
        height: position?.height ?? 0,
        minWidth: Math.min(320, position?.width ?? 320),
        minHeight: Math.min(280, position?.height ?? 280),
        maxWidth: windowMode ? '100vw' : 'calc(100vw - 1.5rem)',
        maxHeight: windowMode ? '100vh' : 'calc(100vh - 1.5rem)',
        resize: windowMode ? 'none' : 'both',
        visibility: position ? 'visible' : 'hidden',
      }}
    >
      <header className="flex flex-none items-start gap-2 border-b border-base-content/15 px-3 py-2" onPointerDown={(event) => {
        if (windowMode && event.button === 0 && !(event.target as HTMLElement).closest('button')) void getCurrentWindow().startDragging();
      }}>
        {history.length > 1 ? (
          <Button
            appearance="ghost"
            size="sm"
            shape="circle"
            className="text-base-content"
            aria-label={t('back')}
            onClick={() => setHistory((previous) => previous.slice(0, -1))}
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
        ) : (
          <BookOpen className="mt-2 h-5 w-5 flex-none text-info" />
        )}

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0">
            <h2 className="break-words text-3xl font-bold leading-none text-info sm:text-4xl">{headExpression}</h2>
            {headReading ? <span className="text-lg text-base-content/70">【{headReading}】</span> : null}
          </div>
        </div>

        <div className="flex flex-none items-center gap-1">
          {headEntry ? <Button appearance="ghost" size="sm" shape="circle" className="text-base-content" aria-label={t('mineToAnki')} disabled={mining !== null} onClick={() => {
            const id = `${headEntry.expression}-${headEntry.reading}`;
            setMining(id); setMiningError(null);
            void ipc.mineDictionaryEntry(headEntry.expression, headEntry.reading, headEntry.glossaries.map(meaningText).filter(Boolean).join('; '), current.query, miningContext)
              .then(() => setMiningError(t('mined'))).catch((error) => setMiningError(formatMiningError(error))).finally(() => setMining(null));
          }}><Plus className="h-4 w-4" /></Button> : null}
          <Button
            appearance="ghost"
            size="sm"
            shape="square"
            className="text-base-content"
            aria-label={t('audioUnavailable')}
            disabled
          >
            <Volume2 className="h-4 w-4" />
          </Button>
          <Button
            appearance="ghost"
            size="sm"
            shape="circle"
            className="text-base-content"
            aria-label={t('close')}
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </header>
      {windowMode && <button type="button" className="absolute bottom-0 right-0 z-10 cursor-se-resize p-1" aria-label={t('resize')} onPointerDown={(event) => {
        event.preventDefault();
        void getCurrentWindow().startResizeDragging('SouthEast');
      }}><Grip className="h-4 w-4" /></button>}

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2 sm:px-3 sm:py-3 selectable">
        {miningError ? <p role="status" className="mb-2 text-xs text-info">{miningError}</p> : null}
        {loading ? <p className="px-1 py-2 text-sm text-base-content/65">{t('loading')}</p> : null}
        {!loading && entries?.length === 0 ? (
          <p className="px-1 py-2 text-sm text-base-content/65">{t('noResults', { term: current.query })}</p>
        ) : null}

        <div className="space-y-1.5">
          {entries?.map((entry, index) => {
            const theme = CARD_THEMES[index % CARD_THEMES.length];
            const tags = entry.tags.split(/\s+/).filter(Boolean);
            return (
              <article
                key={`${entry.dictionary}-${entry.expression}-${entry.reading}-${index}`}
                className={`border-l-4 px-3 py-2.5 sm:px-4 ${theme.card}`}
              >
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className={`badge ${theme.badge} badge-sm font-semibold`}>{entry.dictionary || t('unknownDictionary')}</span>
                  {tags.map((tag) => (
                    <span className="badge badge-ghost badge-sm" key={tag}>
                      {tag}
                    </span>
                  ))}
                  {entry.frequencies.map((frequency, frequencyIndex) => (
                    <span className="badge badge-outline badge-sm" key={`${frequency.dictionary}-${frequency.display}-${frequencyIndex}`}>
                      {frequency.display || frequency.value}
                    </span>
                  ))}
                </div>

                <div className="mt-1 font-semibold leading-6">
                  {entry.expression}
                  {entry.reading && entry.reading !== entry.expression ? (
                    <span className="ml-2 font-normal text-base-content/75">【{entry.reading}】</span>
                  ) : null}
                </div>

                {entry.deinflected && entry.deinflected !== entry.expression ? (
                  <p className="text-xs text-base-content/65">
                    {t('deinflected')}: {entry.deinflected}
                    {entry.process.length > 0 ? ` · ${entry.process.join(' → ')}` : ''}
                  </p>
                ) : null}

                <div className="mt-1 space-y-1 text-sm leading-6">
                  {entry.glossaries.map((glossary, glossaryIndex) => (
                    <StructuredContent
                      key={`${entry.dictionary}-${entry.expression}-${glossaryIndex}`}
                      glossary={glossary as Glossary}
                      onLookup={push}
                      className="text-sm leading-6"
                    />
                  ))}
                </div>
              </article>
            );
          })}
        </div>
      </div>
    </aside>
  );
}
