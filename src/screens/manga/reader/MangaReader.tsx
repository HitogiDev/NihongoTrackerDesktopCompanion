import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode, type WheelEvent as ReactWheelEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { BookX, EyeOff, Loader2, PencilLine, ScanText, Type } from 'lucide-react';
import { DictionaryPopup } from '../../../components/DictionaryPopup';
import { useDictionaryHover } from '../../../hooks/useDictionaryHover';
import { useAppStore } from '../../../store/useAppStore';
import { toast } from '../../../store/useToastStore';
import { Button } from '../../../components/ui/Button';
import { Spinner } from '../../../components/ui/Spinner';
import { events, ipc } from '../../../lib/ipc';
import { mangaUrl } from '../../../lib/manga';
import type { EditedBlock, MangaSessionEnded, MokuroPage, OcrPage, ReaderPrefs, ReaderTarget, ReaderVolume, TextLayerMode } from '../../../lib/types';
import { OcrInstallModal } from '../ocr/OcrInstallModal';
import { ocrErrorText, useOcrInstallStore, useOcrStatus } from '../ocr/useOcr';
import { BlockEditor } from './BlockEditor';
import { EditLayer } from './EditLayer';
import { buildSpreads, clickAction, DEFAULT_PAGE, keyAction, preloadPages, spreadOf, type Size } from './layout';
import { PagedView } from './PagedView';
import { BottomBar, EndCard, SettingsPanel, TopBar } from './ReaderChrome';
import { BACKGROUND_CLASS } from './styles';
import { TextLayer, type HoveredBlock } from './TextLayer';
import { useAutoHide } from './useAutoHide';
import { useMangaSession } from '../useMangaSession';
import { SessionIndicator } from './SessionIndicator';
import { VerticalStrip, type StripHandle } from './VerticalStrip';
import { openVolume } from '../useMangaLibrary';

/** Progress is saved this long after the last page turn, and when closing. */
const SAVE_DELAY_MS = 600;
/** One page per wheel gesture: a trackpad sends dozens of events. */
const WHEEL_GAP_MS = 280;
/** Wider than this (width / height) is a two-page scan that stands alone. */
const WIDE_RATIO = 1.15;
const NEXT_TEXT_MODE: Record<TextLayerMode, TextLayerMode> = { hover: 'always', always: 'hidden', hidden: 'hover' };
const TEXT_MODE_ICON = { hover: ScanText, always: Type, hidden: EyeOff } as const;

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
    // Where a session's log went, when it ends with the reader still open
    // (idle too long, or the next volume opened).
    const offEnded = listen<MangaSessionEnded>(events.mangaSessionEnded, ({ payload }) => {
      if (payload.outcome === 'pending') toast.info(t('session.endedPending', { title: payload.title }));
      else if (payload.outcome === 'logged') toast.success(t('session.endedLogged', { title: payload.title }));
    });
    return () => {
      alive = false;
      void off.then((fn) => fn());
      void offEnded.then((fn) => fn());
    };
  }, [t]);

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
  const { t, i18n } = useTranslation(['mangaReader', 'mining']);
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

  // ── text ───────────────────────────────────────────────────────────
  // Asked for even without a .mokuro: the OCR or a correction may have
  // given some pages text.
  const queryClient = useQueryClient();
  const textQuery = useQuery({
    queryKey: ['manga', 'text', volume.id],
    queryFn: () => ipc.mangaVolumeText(volume.id),
    staleTime: Infinity,
  });
  const text: (MokuroPage | null)[] = useMemo(() => textQuery.data ?? [], [textQuery.data]);
  const hasText = volume.ocr_state !== 'none' || text.some(Boolean);
  const popupDelay = useAppStore((s) => Math.max(0, Math.min(2000, s.settings?.dictionary_popup_delay_ms ?? 75)));
  const theme = useAppStore((s) => s.settings?.theme);
  const dictionary = useDictionaryHover(popupDelay);
  const hoveredBlock = useRef<HoveredBlock | null>(null);
  const copyingBlock = useRef<string | null>(null);
  /** Selects a block's lines for the `copy` handler below to write. */
  const selectForCopy = useCallback((block: HoveredBlock) => {
    const lines = block.element.querySelectorAll('.manga-line');
    const range = document.createRange();
    if (lines.length > 0) {
      range.setStartBefore(lines[0]);
      range.setEndAfter(lines[lines.length - 1]);
    } else {
      range.selectNodeContents(block.element);
    }
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    copyingBlock.current = block.text;
  }, []);
  const copyBlock = useCallback((block: HoveredBlock) => {
    selectForCopy(block);
    // Runs the `copy` handler now, with the block's text.
    if (!document.execCommand('copy')) copyingBlock.current = null;
  }, [selectForCopy]);

  // ── mining ─────────────────────────────────────────────────────────
  // Rust stores the block as a line, mines it and crops the panel around it
  // from the page (`manga::session::mine_block`).
  const [mining, setMining] = useState<string | null>(null);
  const contentId = series.media_id || `manga:${series.id}`;
  const lineContext = useCallback(
    (page: number) => (block: number) => ({ lineId: `manga:${volume.id}:${page}:${block}`, contentId }),
    [contentId, volume.id],
  );
  const mineBlock = useCallback(async (block: HoveredBlock) => {
    const key = `${block.page}:${block.block}`;
    if (mining === key) return;
    setMining(key);
    try {
      const sentence = await ipc.mangaMineBlock(volume.id, block.page, block.block);
      if (sentence.screenshot_path) toast.success(t('mine.done'));
      else toast.warning(t('mine.noPicture'));
    } catch (error) {
      const code = String(error);
      toast.error(i18n.exists(`mining:${code}`) ? t(`mining:${code}`) : t('mine.failed', { message: code }));
    } finally {
      setMining(null);
    }
  }, [i18n, mining, t, volume.id]);
  useEffect(() => {
    const onCopy = (event: ClipboardEvent) => {
      const text = copyingBlock.current;
      if (text === null) return;
      copyingBlock.current = null;
      event.preventDefault();
      event.clipboardData?.setData('text/plain', text);
      window.getSelection()?.removeAllRanges();
      toast.success(t('text.copied'));
    };
    document.addEventListener('copy', onCopy);
    return () => document.removeEventListener('copy', onCopy);
  }, [t]);

  // A page's size is known from its text before its image arrives.
  const sizeOf = useCallback((index: number): Size => {
    const known = natural[index];
    if (known) return known;
    const page = text[index];
    return page && page.img_width > 0 && page.img_height > 0 ? { width: page.img_width, height: page.img_height } : DEFAULT_PAGE;
  }, [natural, text]);
  const rememberSize = useCallback((index: number, size: Size) => {
    setNatural((known) => (known[index]?.width === size.width && known[index]?.height === size.height ? known : { ...known, [index]: size }));
  }, []);
  const spreads = useMemo(
    () => buildSpreads(count, prefs.layout, prefs.cover_alone, (index) => {
      const size = sizeOf(index);
      return size.width > size.height * WIDE_RATIO;
    }),
    [count, prefs.layout, prefs.cover_alone, sizeOf],
  );
  const spreadIndex = spreadOf(spreads, page);
  const spread = spreads[spreadIndex] ?? [0];
  const lastShown = vertical ? page : Math.max(...spread);
  const finished = atEnd || lastShown >= count - 1;

  // ── OCR ────────────────────────────────────────────────────────────
  // Pages on screen without text are read as soon as they show (and the next
  // ones after them), when the OCR is ready (owocr chosen, or the built-in
  // one installed) and the user wants it so.
  const ocr = useOcrStatus().data;
  const autoOcr = useAppStore((s) => s.settings?.manga_ocr_auto ?? true);
  const showInstall = useOcrInstallStore((s) => s.show);
  const [ocrPending, setOcrPending] = useState<Set<number>>(() => new Set());
  const asked = useRef(new Set<number>());
  const shownPages = useMemo(() => (atEnd ? [] : vertical ? [page] : spread), [atEnd, vertical, page, spread]);
  const textLoaded = textQuery.isSuccess;
  const missing = useCallback((index: number) => textLoaded && index >= 0 && index < count && !text[index], [count, text, textLoaded]);
  const readPages = useCallback((visible: number[], ahead: number[]) => {
    const fresh = (pages: number[]) => pages.filter((index) => missing(index) && !asked.current.has(index));
    const now = fresh(visible);
    const later = fresh(ahead);
    if (now.length === 0 && later.length === 0) return;
    for (const index of [...now, ...later]) asked.current.add(index);
    setOcrPending((pending) => new Set([...pending, ...now, ...later]));
    ipc.mangaOcrReadPages(volume.id, now, later).catch((error) => {
      for (const index of [...now, ...later]) asked.current.delete(index);
      setOcrPending(new Set());
      toast.error(ocrErrorText(error));
    });
  }, [missing, volume.id]);
  const shownKey = shownPages.join(',');
  useEffect(() => {
    if (!ocr?.ready || !autoOcr || !textLoaded) return;
    const last = shownPages.length > 0 ? Math.max(...shownPages) : -1;
    const ahead = Array.from({ length: vertical ? 2 : 4 }, (_, offset) => last + 1 + offset);
    readPages(shownPages, ahead);
  }, [ocr?.ready, autoOcr, textLoaded, shownKey, vertical, readPages]);
  useEffect(() => {
    const off = listen<OcrPage>(events.mangaOcrPage, ({ payload }) => {
      if (payload.volume_id !== volume.id) return;
      setOcrPending((pending) => {
        const next = new Set(pending);
        next.delete(Number(payload.page));
        return next;
      });
      void queryClient.invalidateQueries({ queryKey: ['manga', 'text', volume.id] });
    });
    return () => void off.then((fn) => fn());
  }, [queryClient, volume.id]);
  const readingShown = shownPages.some((index) => ocrPending.has(index));
  const shownWithoutText = shownPages.some((index) => missing(index));

  // ── correcting text by hand ────────────────────────────────────────
  const [editing, setEditing] = useState(false);
  const [drafts, setDrafts] = useState<Record<number, EditedBlock[]>>({});
  const [selection, setSelection] = useState<{ page: number; block: number | null } | null>(null);
  const [savingEdits, setSavingEdits] = useState(false);
  const [recognizing, setRecognizing] = useState(false);
  const blocksOf = useCallback((index: number): EditedBlock[] => drafts[index] ?? (text[index]?.blocks ?? []).map((block) => ({
    box: block.box,
    vertical: block.vertical,
    text: block.lines.join('\n'),
  })), [drafts, text]);
  const editPage = (index: number, change: (blocks: EditedBlock[]) => EditedBlock[]) => {
    setDrafts((current) => ({ ...current, [index]: change(current[index] ?? blocksOf(index)) }));
  };
  const selectedBlock = selection && selection.block !== null ? blocksOf(selection.page)[selection.block] ?? null : null;
  const dirtyPages = Object.keys(drafts).map(Number);
  const recognize = async (index: number, block: number, bbox: EditedBlock['box']) => {
    setRecognizing(true);
    try {
      const read = await ipc.mangaOcrRegion(volume.id, index, bbox);
      editPage(index, (blocks) => blocks.map((item, i) => (i === block ? { ...item, text: read } : item)));
    } catch (error) {
      toast.error(ocrErrorText(error));
    } finally {
      setRecognizing(false);
    }
  };
  const saveEdits = async () => {
    setSavingEdits(true);
    try {
      for (const index of dirtyPages) {
        const size = sizeOf(index);
        const own = text[index];
        await ipc.mangaSavePageText(
          volume.id,
          index,
          own && own.img_width > 0 ? own.img_width : size.width,
          own && own.img_height > 0 ? own.img_height : size.height,
          drafts[index],
        );
      }
      if (dirtyPages.length > 0) toast.success(t('edit.saved'));
      setDrafts({});
      setSelection(null);
      setEditing(false);
      await queryClient.invalidateQueries({ queryKey: ['manga', 'text', volume.id] });
    } catch (error) {
      toast.error(t('edit.saveFailed', { message: String(error) }));
    } finally {
      setSavingEdits(false);
    }
  };
  const toggleEditing = () => {
    if (editing) void saveEdits();
    else {
      setEditing(true);
      setSelection(null);
    }
  };

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

  // ── counting ───────────────────────────────────────────────────────
  // Rust times the pages and counts them (`manga::session`); the reader only
  // says which ones are on screen.
  const session = useMangaSession();
  const ownSession = session?.volume_id === volume.id ? session : null;
  const [tracked, setTracked] = useState(false);
  useEffect(() => {
    let alive = true;
    setTracked(false);
    void ipc.mangaSessionOpen(volume.id).then(() => { if (alive) setTracked(true); }).catch(() => undefined);
    return () => { alive = false; };
  }, [volume.id]);
  const onScreen = atEnd ? '' : (vertical ? [page] : spread).join(',');
  useEffect(() => {
    // Only once Rust has the volume: the pages of another one are ignored.
    if (!tracked) return;
    const pages = onScreen ? onScreen.split(',').map(Number) : [];
    void ipc.mangaSessionPages(volume.id, pages).catch(() => undefined);
  }, [tracked, onScreen, volume.id]);

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
    if (nextVolume) void openVolume(nextVolume.id, 0);
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
      // Ctrl+C with nothing selected copies the block under the pointer: it
      // is selected for the copy (no clipboard permission needed in any
      // webview) and the `copy` handler below writes its lines joined.
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'c' && !window.getSelection()?.toString() && hoveredBlock.current) {
        selectForCopy(hoveredBlock.current);
        return;
      }
      // Ctrl+M mines the block under the pointer, as its button does.
      if ((event.ctrlKey || event.metaKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'm') {
        event.preventDefault();
        if (hoveredBlock.current) void mineBlock(hoveredBlock.current);
        else toast.info(t('mine.hoverFirst'));
        return;
      }
      if (!event.ctrlKey && !event.metaKey && !event.altKey && event.key.toLowerCase() === 'e') {
        event.preventDefault();
        toggleEditing();
        return;
      }
      if (editing && (event.key === 'Delete' || event.key === 'Backspace') && selection && selection.block !== null) {
        event.preventDefault();
        const { page: index, block } = selection;
        editPage(index, (blocks) => blocks.filter((_, i) => i !== block));
        setSelection({ page: index, block: null });
        return;
      }
      if (editing && event.key === 'Escape') {
        event.preventDefault();
        if (selection?.block !== null && selection) setSelection({ page: selection.page, block: null });
        else void saveEdits();
        return;
      }
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
        case 'text': if (hasText) updatePrefs({ text_layer: NEXT_TEXT_MODE[prefs.text_layer] }); break;
        case 'escape':
          if (dictionary.dictionaryRequest) dictionary.closeDictionary();
          else if (settingsOpen) setSettingsOpen(false);
          else if (fullscreen) void toggleFullscreen();
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [count, dictionary, fullscreen, goNext, mineBlock, selectForCopy, goPrevious, goSpread, hasText, prefs.direction, prefs.layout, prefs.text_layer, seek, settingsOpen, spreads.length, t, toggleFullscreen, vertical, editing, selection, drafts, text]);

  const lastWheel = useRef(0);
  const onWheel = (event: ReactWheelEvent) => {
    if (vertical || prefs.fit !== 'height' || Math.abs(event.deltaY) < 4) return;
    // The dictionary and the settings scroll on their own.
    if ((event.target as HTMLElement).closest('[role="dialog"], [data-reader-chrome]')) return;
    const now = performance.now();
    if (now - lastWheel.current < WHEEL_GAP_MS) return;
    lastWheel.current = now;
    if (event.deltaY > 0) goNext();
    else goPrevious();
  };

  const onClick = (event: ReactMouseEvent) => {
    const element = event.target as HTMLElement;
    if (element.closest('[data-reader-chrome], [data-text-block], [data-edit-layer], [role="dialog"]')) return;
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

  const overlay = editing
    ? (index: number, size: Size) => {
      const own = text[index];
      return (
        <EditLayer
          pageIndex={index}
          blocks={blocksOf(index)}
          size={size}
          imageWidth={own && own.img_width > 0 ? own.img_width : sizeOf(index).width}
          selected={selection?.page === index ? selection.block : null}
          onSelect={(pageIndex, block) => setSelection({ page: pageIndex, block })}
          onDraw={(pageIndex, box) => {
            const fresh = blocksOf(pageIndex).length;
            const vertical = box[3] - box[1] > box[2] - box[0];
            editPage(pageIndex, (blocks) => [...blocks, { box, vertical, text: '' }]);
            setSelection({ page: pageIndex, block: fresh });
            if (ocr?.ready) void recognize(pageIndex, fresh, box);
          }}
        />
      );
    }
    : hasText
    ? (index: number, size: Size) => (
      <TextLayer
        page={text[index]}
        pageIndex={index}
        size={size}
        mode={prefs.text_layer}
        lineContext={lineContext(index)}
        onPointer={dictionary.openDictionaryAt}
        onLeave={dictionary.cancelDictionaryHover}
        onHoverBlock={(value) => { hoveredBlock.current = value; }}
        onCopy={copyBlock}
        onMine={(block) => void mineBlock(block)}
        mining={mining}
      />
    )
    : undefined;
  const TextModeIcon = TEXT_MODE_ICON[prefs.text_layer];

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
          overlay={overlay}
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
          overlay={overlay}
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
        extra={<>
          {ownSession ? <SessionIndicator session={ownSession} /> : null}
          {readingShown ? (
            <span className="badge badge-soft badge-primary gap-1.5" role="status">
              <Loader2 className="h-3 w-3 motion-safe:animate-spin" aria-hidden="true" />
              {t('ocr.reading')}
            </span>
          ) : shownWithoutText && !editing && ocr?.usable ? (
            <Button
              appearance="ghost"
              size="sm"
              title={ocr.ready ? t('ocr.readPageHint') : t('ocr.installHint')}
              onClick={() => {
                if (!ocr.ready) {
                  showInstall();
                  return;
                }
                asked.current.clear();
                readPages(shownPages, []);
              }}
            >
              <ScanText className="h-4 w-4" />
              <span className="hidden md:inline">{t('ocr.readPage')}</span>
            </Button>
          ) : null}
          {count > 0 ? (
            <Button
              appearance={editing ? 'soft' : 'ghost'}
              variant={editing ? 'primary' : undefined}
              size="sm"
              shape="square"
              aria-pressed={editing}
              aria-label={t('edit.toggle')}
              title={`${t('edit.toggle')} (E)`}
              onClick={toggleEditing}
            >
              <PencilLine className="h-4 w-4" />
            </Button>
          ) : null}
          {hasText ? (
          <Button
            appearance="ghost"
            size="sm"
            shape="square"
            aria-label={t('text.toggle', { mode: t(`text.${prefs.text_layer}`) })}
            title={`${t('text.toggle', { mode: t(`text.${prefs.text_layer}`) })} (T)`}
            onClick={() => updatePrefs({ text_layer: NEXT_TEXT_MODE[prefs.text_layer] })}
          >
            <TextModeIcon className="h-4 w-4" />
          </Button>
          ) : null}
        </>}
      />
      {editing ? (
        <BlockEditor
          block={selectedBlock}
          index={selection?.block ?? null}
          count={shownPages.reduce((sum, index) => sum + blocksOf(index).length, 0)}
          dirty={dirtyPages.length > 0}
          saving={savingEdits}
          recognizing={recognizing}
          canRecognize={Boolean(ocr?.ready) && selectedBlock !== null}
          onChange={(patch) => {
            if (!selection || selection.block === null) return;
            const { page: index, block } = selection;
            editPage(index, (blocks) => blocks.map((item, i) => (i === block ? { ...item, ...patch } : item)));
          }}
          onDelete={() => {
            if (!selection || selection.block === null) return;
            const { page: index, block } = selection;
            editPage(index, (blocks) => blocks.filter((_, i) => i !== block));
            setSelection({ page: index, block: null });
          }}
          onRecognize={() => {
            if (selection && selection.block !== null && selectedBlock) void recognize(selection.page, selection.block, selectedBlock.box);
          }}
          onDiscard={() => {
            setDrafts({});
            setSelection(null);
          }}
          onDone={() => void saveEdits()}
        />
      ) : null}
      <OcrInstallModal />
      {settingsOpen ? <SettingsPanel prefs={prefs} onChange={updatePrefs} hasText={hasText} /> : null}
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
      {dictionary.dictionaryRequest ? (
        <DictionaryPopup
          initial={dictionary.dictionaryRequest}
          miningContext={dictionary.dictionaryContext}
          entries={dictionary.dictionaryEntries}
          loading={dictionary.dictionaryLoading}
          anchor={dictionary.dictionaryAnchor}
          vertical={dictionary.dictionaryVertical}
          theme={theme}
          onLookup={dictionary.lookupDictionary}
          onClose={dictionary.closeDictionary}
        />
      ) : null}
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
