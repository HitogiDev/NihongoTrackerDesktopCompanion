import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode, type WheelEvent as ReactWheelEvent } from 'react';
import { useQuery } from '@tanstack/react-query';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { BookX } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Spinner } from '../../../components/ui/Spinner';
import { events, ipc } from '../../../lib/ipc';
import { mangaUrl } from '../../../lib/manga';
import type { ReaderPrefs, ReaderTarget, ReaderVolume } from '../../../lib/types';
import { buildSpreads, clickAction, DEFAULT_PAGE, keyAction, preloadPages, spreadOf, type Size } from './layout';
import { PagedView } from './PagedView';
import { BottomBar, EndCard, SettingsPanel, TopBar } from './ReaderChrome';
import { BACKGROUND_CLASS } from './styles';
import { useAutoHide } from './useAutoHide';
import { VerticalStrip, type StripHandle } from './VerticalStrip';

/** Progress is saved this long after the last page turn, and when closing. */
const SAVE_DELAY_MS = 600;
/** One page per wheel gesture: a trackpad sends dozens of events. */
const WHEEL_GAP_MS = 280;
/** Wider than this (width / height) is a two-page scan that stands alone. */
const WIDE_RATIO = 1.15;

function useViewport(): Size {
  const [size, setSize] = useState<Size>({ width: window.innerWidth, height: window.innerHeight });
  useEffect(() => {
    const onResize = () => setSize({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return size;
}

function closeWindow() {
  void getCurrentWindow().close();
}

function FullScreenMessage({ children }: { children: ReactNode }) {
  return <div className="manga-bg-black grid h-screen place-items-center p-6 text-center text-neutral-content">{children}</div>;
}

/** The `manga-reader` window: whatever volume Rust points it at. */
export default function MangaReaderWindow() {
  const { t } = useTranslation('mangaReader');
  const [target, setTarget] = useState<ReaderTarget | null | undefined>(undefined);
  const [opened, setOpened] = useState(0);

  useEffect(() => {
    let alive = true;
    void ipc.mangaReaderTarget().then((value) => { if (alive) setTarget(value); });
    const off = listen<ReaderTarget>(events.mangaReaderOpen, (event) => {
      setTarget(event.payload);
      setOpened((count) => count + 1);
    });
    return () => {
      alive = false;
      void off.then((fn) => fn());
    };
  }, []);

  if (target === undefined) return <FullScreenMessage><Spinner size="lg" className="opacity-60" /></FullScreenMessage>;
  if (target === null) {
    return (
      <FullScreenMessage>
        <div>
          <BookX className="mx-auto h-10 w-10 opacity-50" aria-hidden="true" />
          <p className="mt-3 text-sm opacity-80">{t('nothing')}</p>
          <Button size="sm" appearance="outline" className="mt-4" onClick={closeWindow}>{t('close')}</Button>
        </div>
      </FullScreenMessage>
    );
  }
  return <Reader key={`${target.volume_id}:${opened}`} target={target} />;
}

function Reader({ target }: { target: ReaderTarget }) {
  const { t } = useTranslation('mangaReader');
  const query = useQuery({
    queryKey: ['manga', 'reader', target.volume_id],
    queryFn: () => ipc.mangaReaderVolume(target.volume_id),
    staleTime: Infinity,
  });
  if (query.isPending) return <FullScreenMessage><Spinner size="lg" className="opacity-60" /></FullScreenMessage>;
  if (query.isError || !query.data) {
    return (
      <FullScreenMessage>
        <div>
          <BookX className="mx-auto h-10 w-10 opacity-50" aria-hidden="true" />
          <p className="mt-3 text-sm opacity-80">{query.isError ? t('loadFailed', { message: String(query.error) }) : t('gone')}</p>
          <Button size="sm" appearance="outline" className="mt-4" onClick={closeWindow}>{t('close')}</Button>
        </div>
      </FullScreenMessage>
    );
  }
  const { volume } = query.data;
  const count = Number(volume.page_count);
  const saved = volume.finished ? 0 : Number(volume.page ?? 0);
  const initial = Math.min(Math.max(0, Number(target.page ?? saved)), Math.max(0, count - 1));
  return <ReaderView data={query.data} initialPage={initial} />;
}

function ReaderView({ data, initialPage }: { data: ReaderVolume; initialPage: number }) {
  const { t } = useTranslation('mangaReader');
  const { series, volume, next: nextVolume } = data;
  const count = Number(volume.page_count);
  const viewport = useViewport();
  const chrome = useAutoHide();
  const strip = useRef<StripHandle>(null);

  const [prefs, setPrefs] = useState<ReaderPrefs>(data.prefs);
  const [page, setPage] = useState(initialPage);
  const [atEnd, setAtEnd] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [natural, setNatural] = useState<Record<number, Size>>({});
  const vertical = prefs.layout === 'vertical';

  const sizeOf = useCallback((index: number) => natural[index] ?? DEFAULT_PAGE, [natural]);
  const rememberSize = useCallback((index: number, size: Size) => {
    setNatural((known) => (known[index]?.width === size.width && known[index]?.height === size.height ? known : { ...known, [index]: size }));
  }, []);
  const spreads = useMemo(
    () => buildSpreads(count, prefs.layout, prefs.cover_alone, (index) => {
      const size = natural[index];
      return Boolean(size && size.width > size.height * WIDE_RATIO);
    }),
    [count, prefs.layout, prefs.cover_alone, natural],
  );
  const spreadIndex = spreadOf(spreads, page);
  const spread = spreads[spreadIndex] ?? [0];
  const lastShown = vertical ? page : Math.max(...spread);
  const finished = atEnd || lastShown >= count - 1;

  // ── preferences ────────────────────────────────────────────────────
  const updatePrefs = (patch: Partial<ReaderPrefs>) => {
    setPrefs((current) => {
      const updated = { ...current, ...patch };
      void ipc.mangaSaveReaderPrefs(series.id, updated).catch(() => undefined);
      return updated;
    });
  };

  // ── progress ───────────────────────────────────────────────────────
  const latest = useRef({ page, finished });
  latest.current = { page, finished };
  const save = useCallback(
    () => ipc.mangaSaveProgress(volume.id, latest.current.page, latest.current.finished).catch(() => undefined),
    [volume.id],
  );
  useEffect(() => {
    const id = window.setTimeout(() => void save(), SAVE_DELAY_MS);
    return () => window.clearTimeout(id);
  }, [page, finished, save]);
  useEffect(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async () => {
      await save();
      // The library shows progress: let the main window know.
      await emit(events.mangaLibrary).catch(() => undefined);
    });
    return () => {
      void save();
      void unlisten.then((fn) => fn());
    };
  }, [save]);

  // ── window ─────────────────────────────────────────────────────────
  useEffect(() => {
    const title = `${series.title} · ${volume.title}`;
    document.title = title;
    void getCurrentWindow().setTitle(title).catch(() => undefined);
  }, [series.title, volume.title]);
  useEffect(() => {
    void getCurrentWindow().isFullscreen().then(setFullscreen).catch(() => undefined);
  }, [viewport.width, viewport.height]);
  const toggleFullscreen = useCallback(async () => {
    const current = getCurrentWindow();
    const next = !(await current.isFullscreen().catch(() => fullscreen));
    await current.setFullscreen(next).catch(() => undefined);
    setFullscreen(next);
  }, [fullscreen]);

  // ── navigation ─────────────────────────────────────────────────────
  const goSpread = useCallback((index: number) => {
    if (index >= spreads.length) {
      setAtEnd(true);
      return;
    }
    setAtEnd(false);
    setPage(spreads[Math.max(0, index)]?.[0] ?? 0);
  }, [spreads]);
  const openNext = useCallback(() => {
    if (nextVolume) void ipc.mangaOpenReader(nextVolume.id, 0);
  }, [nextVolume]);
  const goNext = useCallback(() => {
    if (atEnd) openNext();
    else goSpread(spreadIndex + 1);
  }, [atEnd, goSpread, openNext, spreadIndex]);
  const goPrevious = useCallback(() => {
    if (atEnd) setAtEnd(false);
    else goSpread(spreadIndex - 1);
  }, [atEnd, goSpread, spreadIndex]);
  const seek = useCallback((target: number) => {
    setAtEnd(false);
    setPage(target);
    if (vertical) strip.current?.scrollToPage(target);
  }, [vertical]);

  // Decode the spreads around this one before they are needed.
  const preloaded = useRef(new Map<number, HTMLImageElement>());
  useEffect(() => {
    if (vertical) return;
    for (const index of preloadPages(spreads, spreadIndex)) {
      if (preloaded.current.has(index)) continue;
      const image = new Image();
      image.decoding = 'async';
      image.src = mangaUrl.page(volume.id, index);
      preloaded.current.set(index, image);
      image.decode()
        .then(() => rememberSize(index, { width: image.naturalWidth, height: image.naturalHeight }))
        .catch(() => preloaded.current.delete(index));
    }
  }, [spreads, spreadIndex, vertical, volume.id, rememberSize]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const element = event.target as HTMLElement | null;
      if (element?.closest('input, textarea, select, [role="dialog"] button')) return;
      const action = keyAction(event, prefs.direction, prefs.layout);
      if (!action) return;
      event.preventDefault();
      switch (action) {
        case 'next': goNext(); break;
        case 'previous': goPrevious(); break;
        case 'first': seek(0); break;
        case 'last': vertical ? seek(count - 1) : goSpread(spreads.length - 1); break;
        case 'screenDown': strip.current?.scrollBy(0.85); break;
        case 'screenUp': strip.current?.scrollBy(-0.85); break;
        case 'stepDown': strip.current?.scrollBy(0.15); break;
        case 'stepUp': strip.current?.scrollBy(-0.15); break;
        case 'fullscreen': void toggleFullscreen(); break;
        case 'escape':
          if (settingsOpen) setSettingsOpen(false);
          else if (fullscreen) void toggleFullscreen();
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [count, fullscreen, goNext, goPrevious, goSpread, prefs.direction, prefs.layout, seek, settingsOpen, spreads.length, toggleFullscreen, vertical]);

  const lastWheel = useRef(0);
  const onWheel = (event: ReactWheelEvent) => {
    if (vertical || prefs.fit !== 'height' || Math.abs(event.deltaY) < 4) return;
    const now = performance.now();
    if (now - lastWheel.current < WHEEL_GAP_MS) return;
    lastWheel.current = now;
    if (event.deltaY > 0) goNext();
    else goPrevious();
  };

  const onClick = (event: ReactMouseEvent) => {
    const element = event.target as HTMLElement;
    if (element.closest('[data-reader-chrome], [data-text-block], [role="dialog"]')) return;
    if (settingsOpen) {
      setSettingsOpen(false);
      return;
    }
    if (window.getSelection()?.toString()) return;
    const action = vertical ? 'toggle' : clickAction(event.clientX, window.innerWidth, prefs.direction);
    if (action === 'next') goNext();
    else if (action === 'previous') goPrevious();
    else chrome.toggle();
  };

  const { pin } = chrome;
  useEffect(() => { pin('settings', settingsOpen); }, [pin, settingsOpen]);

  const pageLabel = spread.length > 1 && !vertical
    ? t('spreadOf', { first: spread[0] + 1, last: spread[spread.length - 1] + 1, count })
    : t('pageOf', { page: page + 1, count });

  return (
    <div
      className={`fixed inset-0 overflow-hidden ${BACKGROUND_CLASS[prefs.background]} ${chrome.visible || settingsOpen ? '' : 'cursor-none'}`}
      onClick={onClick}
      onWheel={onWheel}
    >
      {vertical ? (
        <VerticalStrip
          ref={strip}
          volumeId={volume.id}
          pageCount={count}
          initialPage={page}
          fit={prefs.fit}
          viewport={viewport}
          sizeOf={sizeOf}
          onNaturalSize={rememberSize}
          onPage={setPage}
        />
      ) : (
        <PagedView
          volumeId={volume.id}
          spread={spread}
          direction={prefs.direction}
          fit={prefs.fit}
          viewport={viewport}
          sizeOf={sizeOf}
          onNaturalSize={rememberSize}
        />
      )}

      <TopBar
        visible={chrome.visible || settingsOpen}
        title={series.title}
        subtitle={volume.title}
        fullscreen={fullscreen}
        onClose={closeWindow}
        onFullscreen={() => void toggleFullscreen()}
        settingsOpen={settingsOpen}
        onSettings={() => setSettingsOpen((open) => !open)}
        onHover={(hovering) => chrome.pin('top', hovering)}
      />
      {settingsOpen ? <SettingsPanel prefs={prefs} onChange={updatePrefs} hasText={volume.ocr_state !== 'none'} /> : null}
      <BottomBar
        visible={chrome.visible || settingsOpen}
        volumeId={volume.id}
        page={page}
        shown={vertical ? [page] : spread}
        pageLabel={pageLabel}
        pageCount={count}
        direction={prefs.direction}
        onSeek={seek}
        onHover={(hovering) => chrome.pin('bottom', hovering)}
      />
      {atEnd ? (
        <EndCard
          volumeTitle={volume.title}
          next={nextVolume}
          onNext={openNext}
          onRestart={() => seek(0)}
          onClose={closeWindow}
          onDismiss={() => setAtEnd(false)}
        />
      ) : null}
    </div>
  );
}
