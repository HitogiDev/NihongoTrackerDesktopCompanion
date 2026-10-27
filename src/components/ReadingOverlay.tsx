import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { BookOpen, Grip, MousePointer2, Settings, Target, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { LookupRequest } from '@nihongotracker/yomitan-content';
import { useReadingOverlay } from '../hooks/useReadingOverlay';
import { useAppStore } from '../store/useAppStore';
import { ipc } from '../lib/ipc';
import { readingOverlayTextStyle } from '../lib/readingOverlayStyle';
import { textHitAtPoint } from '../lib/textHit';
import type { DictionaryEntry, TextLine } from '../lib/types';
import { DictionaryPopup } from './DictionaryPopup';
import { NativeHookChoices } from './NativeHookChoices';
import { Button } from './ui/Button';

export function ReadingOverlay() {
  const { t } = useTranslation('readingOverlay');
  const view = useReadingOverlay();
  const settings = useAppStore((state) => state.settings);
  const tracker = useAppStore((state) => state.tracker);
  const popupCloseDelay = useRef(500);
  popupCloseDelay.current = settings?.reading_overlay.popup_close_delay_ms ?? 500;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hoverKey = useRef<string | null>(null);
  const hoverPoint = useRef({ x: 0, y: 0 });
  const popupLeaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const line = view?.popup?.line ?? view?.line;
  const [error, setError] = useState<string | null>(null);
  const [hovered, setHovered] = useState(false);
  const latest = useRef({ line, view, popupEnabled: settings?.reading_overlay.popup_enabled });
  latest.current = { line, view, popupEnabled: settings?.reading_overlay.popup_enabled };
  const clearHover = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    hoverKey.current = null;
  }, []);
  const cancelPopupLeave = useCallback(() => {
    if (popupLeaveTimer.current !== null) clearTimeout(popupLeaveTimer.current);
    popupLeaveTimer.current = null;
  }, []);
  const schedulePopupLeave = useCallback(() => {
    if (latest.current.view?.popup?.mode !== 'dictionary') return;
    cancelPopupLeave();
    popupLeaveTimer.current = setTimeout(() => {
      popupLeaveTimer.current = null;
      if (latest.current.view?.popup?.mode === 'dictionary') void ipc.readingOverlayAction('close-popup');
    }, popupCloseDelay.current);
  }, [cancelPopupLeave]);
  useEffect(() => {
    const listener = listen('reading-overlay-popup-enter', cancelPopupLeave);
    return () => { cancelPopupLeave(); void listener.then((stop) => stop()); };
  }, [cancelPopupLeave]);
  useEffect(() => {
    void ipc.readingOverlayAction('ready');
    return clearHover;
  }, [clearHover]);
  useEffect(clearHover, [line?.id, view?.target?.started_at, view?.click_through, view?.visible, clearHover]);
  useEffect(() => {
    if (settings?.reading_overlay.popup_enabled === false) {
      clearHover(); cancelPopupLeave();
    }
  }, [settings?.reading_overlay.popup_enabled, clearHover, cancelPopupLeave]);

  function hover(event: MouseEvent<HTMLParagraphElement>) {
    if (!line || settings?.reading_overlay.popup_enabled === false || view?.click_through || view?.popup?.mode === 'hooks') return;
    const hit = textHitAtPoint(event.currentTarget, event.clientX, event.clientY);
    if (!hit || !String.fromCodePoint(line.text.codePointAt(hit.offset) ?? 0).match(/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u)) {
      clearHover();
      schedulePopupLeave();
      return;
    }
    const { offset, vertical } = hit;
    cancelPopupLeave();
    void emit('reading-overlay-text-enter');
    hoverPoint.current = { x: event.clientX, y: event.clientY };
    const key = `${line.id}:${offset}`;
    if (hoverKey.current === key) return;
    clearHover();
    hoverKey.current = key;
    timer.current = setTimeout(() => {
      timer.current = null;
      if (latest.current.line?.id !== line.id || !latest.current.view?.visible || latest.current.view.click_through || latest.current.popupEnabled === false) return;
      const { x, y } = hoverPoint.current;
      void ipc.readingOverlayPopup('dictionary', line.id, offset, x, y, vertical, { top: hit.rect.top, bottom: hit.rect.bottom }).catch(() => setError(t('lookupFailed')));
    }, Math.max(0, Math.min(2000, settings?.reading_overlay.lookup_delay_ms ?? 75)));
  }

  const native = tracker?.game_hook;
  const status = native && native.pid === view?.target?.pid && native.content_id === view?.target?.content_id ? native : null;
  const preferences = settings?.reading_overlay;
  const pointerInside = view?.click_through ? view.toolbar_hovered : hovered;
  const idle = Boolean(line) && !view?.reveal_controls && !pointerInside && !view?.popup;
  const textVisible = preferences?.text_visibility !== 'hover_only' || !idle;
  const overlayOpacity = preferences?.text_visibility === 'dim_when_idle' && idle
    ? (preferences.idle_opacity ?? 35) / 100 : 1;
  const toolbarVisibility = preferences?.toolbar_title_hover_only
    ? 'opacity-0 hover:opacity-100 focus-within:opacity-100'
    : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100';
  return <div className="group relative flex h-screen flex-col overflow-hidden rounded-box text-base-content transition-opacity duration-200"
    onPointerEnter={() => { setHovered(true); cancelPopupLeave(); }}
    onPointerLeave={() => {
      setHovered(false);
      clearHover();
      schedulePopupLeave();
    }}
    style={{ backgroundColor: `color-mix(in srgb, var(--color-base-100) ${preferences?.opacity ?? 65}%, transparent)`, opacity: overlayOpacity }}>
    <div aria-label={t('move')} className={`absolute inset-x-0 top-0 z-10 flex cursor-move items-center gap-1 px-2 transition-opacity ${toolbarVisibility}`}
      style={{ height: 32, opacity: view?.reveal_controls || !line || (view?.click_through && view.toolbar_hovered) ? 1 : undefined }}
      onPointerDown={(event) => {
        if (event.button === 0 && !(event.target as HTMLElement).closest('button')) {
          clearHover(); void getCurrentWindow().startDragging();
        }
      }}>
      <span className="min-w-0 flex-1 truncate text-xs">{view?.target?.title}</span>
      <Button appearance="ghost" size="xs" shape="square" title={t('openTexthooker')} aria-label={t('openTexthooker')}
        onClick={() => void ipc.readingOverlayAction('show').then(() => ipc.openAuxWindow('texthooker'))}><BookOpen className="h-4 w-4" /></Button>
      <Button appearance="ghost" size="xs" shape="square" title={t('hooks')} aria-label={t('hooks')}
        onClick={(event) => void ipc.readingOverlayPopup('hooks', null, 0, event.clientX, event.clientY).catch(() => setError(t('lookupFailed')))}>
        <Target className={status?.phase === 'receiving' ? 'h-4 w-4 text-success' : 'h-4 w-4 text-warning'} />
      </Button>
      <Button appearance="ghost" size="xs" shape="square" title={t('settings')} aria-label={t('settings')} onClick={() => void ipc.readingOverlayAction('settings')}><Settings className="h-4 w-4" /></Button>
      <Button appearance="ghost" size="xs" shape="square" title={t('clickThrough', { shortcut: settings?.reading_overlay.interaction_shortcut })} aria-label={t('clickThrough', { shortcut: settings?.reading_overlay.interaction_shortcut })}
        aria-pressed={view?.click_through ?? false}
        onClick={() => void ipc.readingOverlayAction('interaction')}><MousePointer2 className="h-4 w-4" /></Button>
      <Button appearance="ghost" size="xs" shape="square" title={t('hide')} aria-label={t('hide')} onClick={() => void ipc.readingOverlayAction('hide')}><X className="h-4 w-4" /></Button>
    </div>
    <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4 pt-9">
      {line ? <p lang="ja" className="selectable whitespace-pre-wrap break-words leading-relaxed transition-opacity duration-200" onMouseMove={hover} onMouseLeave={() => { clearHover(); schedulePopupLeave(); }}
        style={{ ...readingOverlayTextStyle(preferences),
          textAlign: preferences?.center_text ? 'center' : 'left', opacity: textVisible ? 1 : 0 }}>
        {line.text}
      </p> : <p className="text-sm text-base-content/70">{t(status?.selected_hook_code ? 'waiting' : 'chooseHook')}</p>}
      {error && <p role="status" className="mt-2 text-xs text-error">{error}</p>}
    </div>
    <button type="button" className="absolute bottom-0 right-0 cursor-se-resize p-1 opacity-0 group-hover:opacity-100" aria-label={t('resize')} onPointerDown={(event) => {
      event.preventDefault(); clearHover(); void getCurrentWindow().startResizeDragging('SouthEast');
    }}><Grip className="h-4 w-4" /></button>
  </div>;
}

function OverlayDictionary({ line, offset }: { line: TextLine; offset: number }) {
  const preferences = useAppStore((state) => state.settings?.reading_overlay);
  const [entries, setEntries] = useState<DictionaryEntry[]>();
  const [loading, setLoading] = useState(false);
  const requestId = useRef(0);
  const initial = useRef<LookupRequest>({ query: line.text, offset }).current;
  const lookup = useCallback(async (request: LookupRequest) => {
    const id = ++requestId.current;
    setLoading(true);
    setEntries(undefined);
    try {
      const result = await ipc.dictionaryLookup(request.query, request.offset ?? 0);
      if (id === requestId.current) setEntries(result.entries);
    } catch {
      if (id === requestId.current) setEntries([]);
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }, []);
  useEffect(() => () => { requestId.current += 1; }, []);
  return <DictionaryPopup initial={initial} entries={entries} loading={loading} anchor={null} windowMode
    backgroundOpacity={preferences?.popup_background_opacity ?? 100} closeDelayMs={preferences?.popup_close_delay_ms ?? 500}
    miningContext={{ lineId: line.id, contentId: line.content_id }} onLookup={lookup}
    onClose={() => void ipc.readingOverlayAction('close-popup')} />;
}

export function ReadingOverlayPopup() {
  const { t } = useTranslation(['texthooker', 'readingOverlay', 'common']);
  const view = useReadingOverlay();
  const tracker = useAppStore((state) => state.tracker);
  const [selected, setSelected] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const target = view?.target;
  const native = tracker?.game_hook;
  const status = native?.pid === target?.pid && native?.content_id === target?.content_id ? native ?? null : null;
  const popup = view?.popup;
  const liveTarget = useRef(target);
  liveTarget.current = target;
  useEffect(() => { void ipc.readingOverlayAction('ready', true); }, []);
  useEffect(() => { setSelected(null); setError(null); setSaving(false); }, [target?.started_at, target?.pid, popup?.mode]);
  const selection = selected ?? status?.selected_hook_code ?? null;
  const close = () => void ipc.readingOverlayAction('close-popup');

  async function save() {
    if (!target || !selection) return;
    setSaving(true); setError(null);
    try {
      await ipc.readingOverlaySelectHook(target.pid, target.content_id, target.started_at, selection);
      if (liveTarget.current?.started_at === target.started_at && liveTarget.current.pid === target.pid) close();
    } catch {
      setError(t('hookSelection.saveFailed'));
    } finally { setSaving(false); }
  }

  if (!popup) return null;
  if (popup.mode === 'dictionary' && popup.line) return <OverlayDictionary key={`${popup.line.id}:${popup.offset}`} line={popup.line} offset={popup.offset} />;
  return <section role="dialog" aria-label={t('hookSelection.title')} className="surface flex h-screen flex-col overflow-hidden p-4 text-base-content">
    <header className="mb-3 flex items-center gap-2" onPointerDown={(event) => {
      if (event.button === 0 && !(event.target as HTMLElement).closest('button')) void getCurrentWindow().startDragging();
    }}>
      <h2 className="flex-1 font-semibold">{t('hookSelection.title')}</h2>
      <Button appearance="ghost" size="sm" shape="circle" aria-label={t('common:close')} onClick={close}><X className="h-4 w-4" /></Button>
    </header>
    <div className="min-h-0 flex-1 overflow-y-auto"><NativeHookChoices status={status} title={target?.title} selected={selection} onSelect={setSelected} />
      {error && <p role="status" className="mt-2 text-sm text-error">{error}</p>}
    </div>
    <footer className="mt-3 flex justify-end gap-2">
      <Button appearance="ghost" onClick={close}>{t('common:cancel')}</Button>
      <Button variant="primary" loading={saving} disabled={!status?.hooks.some((hook) => hook.selection_key === selection)} onClick={() => void save()}>{t('hookSelection.save')}</Button>
    </footer>
    <button type="button" className="absolute bottom-0 right-0 cursor-se-resize p-1" aria-label={t('readingOverlay:resize')} onPointerDown={(event) => {
      event.preventDefault(); void getCurrentWindow().startResizeDragging('SouthEast');
    }}><Grip className="h-3 w-3" /></button>
  </section>;
}
