import { NativeHookTextFilters } from '../components/NativeHookTextFilters';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import {
  Activity,
  Brain,
  Copy,
  HelpCircle,
  Inbox,
  Monitor,
  Settings,
  Trash2,
  Share2,
  X,
} from 'lucide-react';
import { Field } from '../components/ui/Field';
import { Button } from '../components/ui/Button';
import { Modal } from '../components/ui/Modal';
import { DictionaryPopup } from '../components/DictionaryPopup';
import { NativeHookChoices } from '../components/NativeHookChoices';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { formatClock, numberWithCommas } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { miningError } from '../lib/mining';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from '../store/useAppStore';
import { useManualTimer } from './texthooker/useManualTimer';
import { useHookerLines } from './texthooker/useHookerLines';
import { useRoom } from './texthooker/useRoom';
import { useDictionaryHover } from './texthooker/useDictionaryHover';
import { LineList } from './texthooker/LineList';
import { TopBar } from './texthooker/TopBar';
import { TimerEditModal, TimerResetModal } from './texthooker/TimerEditModal';
import { StatsModal } from './texthooker/StatsModal';
import {
  FONT_FAMILY_OPTIONS,
  HOOKER_THEME_OPTIONS,
  LOCAL_SESSION,
} from './texthooker/constants';
import { randomRoomId } from '../lib/room';
import type { TextSessionSummary } from '../lib/types';


/** `vn://ended`: where a detected session went once the game closed. */
interface EndedSession {
  minutes: number;
  chars: number;
  outcome: 'pending' | 'logged' | 'discarded';
}


export default function TexthookerScreen() {
  const { t } = useTranslation(['texthooker', 'common']);
  const queryClient = useQueryClient();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const hooker = useAppStore((s) => s.hooker);

  const contentId = hooker?.content_id ?? LOCAL_SESSION;
  const [miningFeedback, setMiningFeedback] = useState<string | null>(null);
  const [endedSession, setEndedSession] = useState<EndedSession | null>(null);

  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<'display' | 'behavior' | 'intelligence'>('display');
  const [isStatsOpen, setIsStatsOpen] = useState(false);
  const [isTimerEditOpen, setIsTimerEditOpen] = useState(false);
  const [isResetTimerConfirmOpen, setIsResetTimerConfirmOpen] = useState(false);
  const [clearTarget, setClearTarget] = useState<string | null>(null);
  const [clearingLines, setClearingLines] = useState(false);
  const [clearError, setClearError] = useState(false);
  const [isClassHelpOpen, setIsClassHelpOpen] = useState(false);
  const [isSessionPickerOpen, setIsSessionPickerOpen] = useState(false);
  const [isHookPickerOpen, setIsHookPickerOpen] = useState(false);
  const [selectedHookCode, setSelectedHookCode] = useState<string | null>(null);
  const [savingHook, setSavingHook] = useState(false);
  const [changingInput, setChangingInput] = useState(false);
  const [inputSourceError, setInputSourceError] = useState(false);
  const inputSourceRequestRef = useRef(0);
  const [hookSaveError, setHookSaveError] = useState<string | null>(null);
  const hookPickerOpenedForPidRef = useRef<number | null>(null);
  const [attaching, setAttaching] = useState(false);

  const settingsPanelRef = useRef<HTMLDivElement>(null);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);

  // A detected visual novel drives the clock from Rust; the manual timer is for
  // everything else. Only the fields read here: the snapshot changes every
  // second while a session counts, and the clock is not one of them.
  const autoSession = useAppStore(useShallow((s) => {
    const vn = s.tracker?.vn;
    return vn && vn.content_id === contentId
      ? {
          paused_by_user: vn.paused_by_user,
          session_chars: vn.session_chars,
          session_lines: vn.session_lines,
          image: vn.image,
        }
      : null;
  }));
  const hasAutoSession = Boolean(autoSession);
  const detectedVn = useAppStore(useShallow((s) => s.tracker?.vn
    ? { exe: s.tracker.vn.exe, media_title: s.tracker.vn.media_title }
    : null));
  const gameHook = useAppStore((s) => s.tracker?.game_hook ?? null);
  const nativeHook = gameHook?.exe ? gameHook : null;
  const nativeSession = nativeHook?.content_id === contentId;
  const nativeSelected = Boolean(nativeSession && !nativeHook.websocket_selected);
  const nativeInput = nativeSelected && nativeHook
    && ['starting', 'attached', 'receiving'].includes(nativeHook.phase);
  const hookApps = useQuery({ queryKey: ['vn-apps'], queryFn: ipc.listVnApps, enabled: isHookPickerOpen });
  const hookApp = hookApps.data?.find((entry) => entry.exe.toLowerCase() === detectedVn?.exe.toLowerCase());
  const [enablingHook, setEnablingHook] = useState(false);
  useEffect(() => {
    inputSourceRequestRef.current += 1;
    setChangingInput(false);
    setInputSourceError(false);
  }, [contentId, nativeHook?.pid]);

  const openHookPicker = () => {
    setSelectedHookCode(
      nativeSession && nativeHook?.hooks.some((hook) => hook.selection_key === nativeHook.selected_hook_code)
        ? nativeHook.selected_hook_code
        : null
    );
    setHookSaveError(null);
    setIsHookPickerOpen(true);
  };

  const changeInputSource = async (source: 'lunahook' | 'websocket') => {
    if (changingInput) return;
    const request = ++inputSourceRequestRef.current;
    setChangingInput(true);
    setInputSourceError(false);
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const status = await Promise.race([
        ipc.setHookerInputSource(contentId, source),
        new Promise<never>((_, reject) => {
          timeout = setTimeout(() => reject(new Error('Input source change timed out')), 5000);
        }),
      ]);
      if (request !== inputSourceRequestRef.current) return;
      useAppStore.setState((state) => state.tracker
        ? { tracker: { ...state.tracker, game_hook: status } }
        : {});
      if (source === 'websocket') setIsHookPickerOpen(false);
    } catch {
      if (request === inputSourceRequestRef.current) setInputSourceError(true);
    } finally {
      clearTimeout(timeout);
      if (request === inputSourceRequestRef.current) setChangingInput(false);
    }
  };
  useEffect(() => {
    if (!nativeHook?.pid) {
      hookPickerOpenedForPidRef.current = null;
      return;
    }
    if (nativeHook.selected_hook_code) {
      setSelectedHookCode(nativeHook.selected_hook_code);
    }
    if (nativeInput && !nativeHook.selected_hook_code
      && hookPickerOpenedForPidRef.current !== nativeHook.pid) {
      setSelectedHookCode(null);
      setHookSaveError(null);
      setIsHookPickerOpen(true);
      hookPickerOpenedForPidRef.current = nativeHook.pid;
    }
  }, [nativeHook?.pid, nativeHook?.selected_hook_code, nativeInput]);

  const enableNativeHook = async () => {
    if (!hookApp) return;
    setEnablingHook(true);
    setHookSaveError(null);
    try {
      const apps = await ipc.upsertVnApp({ ...hookApp, native_hook_enabled: true });
      queryClient.setQueryData(['vn-apps'], apps);
    } catch {
      setHookSaveError(t('hookSelection.enableFailed'));
    } finally {
      setEnablingHook(false);
    }
  };

  const saveNativeHookSelection = async () => {
    if (!nativeHook?.exe || !selectedHookCode) return;
    setSavingHook(true);
    setHookSaveError(null);
    try {
      await ipc.selectNativeHook(nativeHook.exe, selectedHookCode);
      setIsHookPickerOpen(false);
    } catch {
      setHookSaveError(t('hookSelection.saveFailed'));
    } finally {
      setSavingHook(false);
    }
  };

  const vertical = settings?.texthooker_vertical ?? false;
  const lineGap = settings?.texthooker_line_gap ?? 16;
  const lineSpacing = settings?.texthooker_line_spacing ?? 1.5;
  const fontSize = settings?.texthooker_font_size ?? 24;
  const fontFamily = settings?.texthooker_font_family ?? 'Noto Sans JP';
  const customCss = settings?.texthooker_custom_css ?? '';
  const hookerTheme = settings?.texthooker_theme ?? 'system';
  const forceTopbarVertical = settings?.texthooker_force_topbar_vertical ?? false;
  const autoPauseTimeout = settings?.texthooker_auto_pause_timeout ?? 120;
  const {
    isTimerActive, setIsTimerActive, secondsRef, lastActivityRef, anchorTimer, setSeconds, clock,
  } = useManualTimer({ contentId, hasAutoSession, autoPauseTimeout });
  const {
    contentIdRef, lines, totals, canUndo, reload,
    handleDelete, handleDeleteLast, handleUndo, pushUndo, handleCopyAll,
  } = useHookerLines({
    contentId,
    hasAutoSession,
    onTimerLoaded: setSeconds,
    onSwitch: () => setIsTimerActive(false),
    onLine: () => {
      void queryClient.invalidateQueries({ queryKey: ['text-context'] });
      lastActivityRef.current = Date.now();
    },
  });
  const {
    roomMode, roomId, inviteLink, roomConnected, roomMembers, roomError, inviteCopied,
    copyInvite, connectRoom, disconnectRoom,
  } = useRoom({ settings, contentId, reload });
  const dictionaryPopupDelay = Math.max(0, Math.min(2000, settings?.dictionary_popup_delay_ms ?? 75));

  const {
    dictionaryRequest, dictionaryContext, dictionaryEntries, dictionaryLoading, dictionaryAnchor,
    dictionaryVertical, lookupDictionary, openDictionaryAtPointer, cancelDictionaryHover, closeDictionary,
  } = useDictionaryHover(dictionaryPopupDelay);

  // Closing the game ends the detected session and its counters drop to zero
  // right away; without saying where it went, the session looks lost.
  useEffect(() => {
    const unlisten = listen<EndedSession & { contentId: string }>(events.vnEnded, (event) => {
      if (event.payload.contentId !== contentId) return;
      setEndedSession(event.payload.outcome === 'discarded' ? null : event.payload);
      void reload();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [contentId, reload]);

  useEffect(() => { if (hasAutoSession) setEndedSession(null); }, [hasAutoSession]);
  useEffect(() => { setEndedSession(null); }, [contentId]);

  const toggleTimer = useCallback(() => {
    if (autoSession) {
      void ipc.setSessionPaused(!autoSession.paused_by_user);
      return;
    }
    setIsTimerActive((prev) => {
      if (!prev) {
        lastActivityRef.current = Date.now();
        anchorTimer(secondsRef.current);
      }
      return !prev;
    });
  }, [autoSession, anchorTimer, setIsTimerActive, lastActivityRef, secondsRef]);

  const handleOpenTimerEdit = useCallback(() => setIsTimerEditOpen(true), []);
  const closeTimerEdit = useCallback(() => setIsTimerEditOpen(false), []);
  const closeResetTimer = useCallback(() => setIsResetTimerConfirmOpen(false), []);

  const handleSaveTimerEdit = useCallback(async (total: number) => {
    await ipc.hookerSetTimer(contentId, total, true);
    if (contentIdRef.current !== contentId) return;
    setSeconds(total);
    lastActivityRef.current = Date.now();
    setIsTimerEditOpen(false);
  }, [contentId, contentIdRef, setSeconds, lastActivityRef]);

  const handleResetTimer = useCallback(async () => {
    if (hasAutoSession) await ipc.setSessionPaused(true);
    await ipc.hookerSetTimer(contentId, 0, true);
    if (contentIdRef.current !== contentId) return;
    setSeconds(0);
    setIsTimerActive(false);
    setIsResetTimerConfirmOpen(false);
  }, [contentId, contentIdRef, hasAutoSession, setSeconds, setIsTimerActive]);

  useEffect(() => { setIsTimerEditOpen(false); setIsResetTimerConfirmOpen(false); }, [contentId]);

  // ── lines ───────────────────────────────────────────────────────────

  const handleClearAll = useCallback(async () => {
    if (lines.length === 0 || clearTarget !== contentId || clearingLines) return;
    setClearingLines(true);
    setClearError(false);
    try {
      await ipc.hookerClear(contentId);
      if (contentIdRef.current !== contentId) return;
      pushUndo(lines);
      setClearTarget(null);
      await reload();
    } catch {
      if (contentIdRef.current === contentId) setClearError(true);
    } finally {
      setClearingLines(false);
    }
  }, [lines, contentId, contentIdRef, clearTarget, clearingLines, reload, pushUndo]);

  useEffect(() => { setClearTarget(null); setClearError(false); }, [contentId]);

  // ── derived ─────────────────────────────────────────────────────────

  const { data: context } = useQuery({
    queryKey: ['text-context', settings?.api_base_url, settings?.username, contentId],
    queryFn: () => ipc.hookerContext(contentId),
  });
  useEffect(() => {
    void ipc.refreshHookerContext(contentId);
  }, [contentId, isStatsOpen, settings?.api_base_url, settings?.username]);
  useEffect(() => {
    const changed = listen<{ contentId: string }>(events.hookerContext, (event) => {
      if (event.payload.contentId === contentId) {
        void queryClient.invalidateQueries({ queryKey: ['text-context'] });
      }
    });
    const outbox = listen(events.outbox, () => {
      void queryClient.invalidateQueries({ queryKey: ['text-context'] });
    });
    return () => { void changed.then((unlisten) => unlisten()); void outbox.then((unlisten) => unlisten()); };
  }, [contentId, queryClient]);
  const linesNumber = totals?.lines ?? lines.length;
  const charsNumber = totals?.chars ?? lines.reduce((sum, line) => sum + line.chars, 0);
  const currentSessionChars = autoSession?.session_chars ?? totals?.sessionChars ?? 0;
  const currentSessionLines = autoSession?.session_lines ?? totals?.sessionLines ?? 0;
  const closeStats = useCallback(() => setIsStatsOpen(false), []);
  const handleSessionLogged = useCallback(async () => {
    setSeconds(0);
    setIsTimerActive(false);
    await reload();
    void queryClient.invalidateQueries({ queryKey: ['outbox'] });
    void queryClient.invalidateQueries({ queryKey: ['text-context'] });
  }, [setSeconds, setIsTimerActive, reload, queryClient]);

  // Close the settings panel on an outside click or Escape, like the web one.
  useEffect(() => {
    if (!isSettingsOpen) return;
    const onMouseDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (settingsButtonRef.current?.contains(target)) return;
      if (!settingsPanelRef.current?.contains(target)) setIsSettingsOpen(false);
    };
    const onEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setIsSettingsOpen(false);
    };
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('keydown', onEscape);
    return () => {
      document.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('keydown', onEscape);
    };
  }, [isSettingsOpen]);

  const systemTheme =
    typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark';
  const effectiveHookerTheme =
    hookerTheme === 'system' ? systemTheme : hookerTheme || undefined;

  async function saveSentence(lineId?: string) {
    try {
      if (lineId) await ipc.mineLine(lineId);
      else await ipc.mineLatestLine();
      setMiningFeedback(t('mining.saved'));
    } catch (error) {
      setMiningFeedback(miningError(error));
    }
  }

  // Stable identities for `LineItem`; they always call the latest closures.
  const lineActions = useRef({ save: saveSentence, remove: handleDelete });
  lineActions.current = { save: saveSentence, remove: handleDelete };
  const saveLine = useCallback((id: string) => void lineActions.current.save(id), []);
  const deleteLine = useCallback((id: string) => void lineActions.current.remove(id), []);

  return (
    <div data-theme={effectiveHookerTheme} className="bg-base-300 min-h-screen th-root">
      {customCss.trim() && <style>{customCss}</style>}
      {endedSession ? (
        <div role="status" className="fixed bottom-3 left-3 z-50 alert alert-success w-auto max-w-md text-sm shadow-lg">
          <Inbox className="w-4 h-4" />
          <span>
            {t(endedSession.outcome === 'pending' ? 'sessionEnded.pending' : 'sessionEnded.logged', {
              minutes: endedSession.minutes,
              chars: numberWithCommas(endedSession.chars),
            })}
          </span>
          {endedSession.outcome === 'pending' ? (
            <button type="button" className="btn btn-xs" onClick={() => { setEndedSession(null); void ipc.overlayOpenMain('/queue'); }}>
              {t('sessionEnded.openQueue')}
            </button>
          ) : null}
          <button type="button" className="btn btn-xs btn-ghost btn-square" aria-label={t('sessionEnded.dismiss')} onClick={() => setEndedSession(null)}>
            <X className="w-4 h-4" />
          </button>
        </div>
      ) : null}
      {miningFeedback ? <button type="button" className="fixed bottom-3 right-3 z-50 alert alert-info w-auto max-w-sm text-sm" onClick={() => setMiningFeedback(null)}>{miningFeedback}</button> : null}

      <TopBar
        settingsButtonRef={settingsButtonRef}
        clock={clock}
        contentId={contentId}
        sessionTitle={hooker?.content_title ?? totals?.mediaTitle ?? t('sessions.free')}
        currentSessionChars={currentSessionChars}
        currentSessionLines={currentSessionLines}
        charsNumber={charsNumber}
        linesNumber={linesNumber}
        nativeHook={nativeHook}
        nativeSession={nativeSession}
        nativeSelected={nativeSelected}
        nativeInput={Boolean(nativeInput)}
        changingInput={changingInput}
        inputSourceError={inputSourceError}
        autoPausedByUser={autoSession ? autoSession.paused_by_user : null}
        isTimerActive={isTimerActive}
        canUndo={canUndo}
        roomConnected={roomConnected}
        roomMembers={roomMembers}
        onChangeInputSource={(source) => void changeInputSource(source)}
        onOpenHookPicker={openHookPicker}
        onToggleTimer={toggleTimer}
        onResetTimer={() => setIsResetTimerConfirmOpen(true)}
        onEditTimer={handleOpenTimerEdit}
        onDeleteLast={handleDeleteLast}
        onUndo={() => void handleUndo()}
        onOpenStats={() => setIsStatsOpen(true)}
        onToggleSettings={() => setIsSettingsOpen((prev) => !prev)}
        onOpenSessions={() => setIsSessionPickerOpen(true)}
      />

      <TimerEditModal
        open={isTimerEditOpen}
        clock={clock}
        onClose={closeTimerEdit}
        onSave={handleSaveTimerEdit}
      />

      <TimerResetModal
        open={isResetTimerConfirmOpen}
        onClose={closeResetTimer}
        onReset={handleResetTimer}
      />

      <StatsModal
        open={isStatsOpen}
        onClose={closeStats}
        clock={clock}
        contentId={contentId}
        sessionTitle={hooker?.content_title ?? null}
        lines={lines}
        totals={totals}
        context={context}
        autoImage={autoSession?.image ?? null}
        linesNumber={linesNumber}
        charsNumber={charsNumber}
        currentSessionChars={currentSessionChars}
        currentSessionLines={currentSessionLines}
        onLogged={handleSessionLogged}
      />

      {/* Settings Dropdown Panel */}
      {isSettingsOpen && (
        <div
          ref={settingsPanelRef}
          className="fixed top-16 right-3 z-[60] w-[min(92vw,52rem)] max-h-[calc(100vh-5rem)] overflow-y-auto rounded-xl border border-base-content/15 bg-base-100/95 backdrop-blur-md shadow-lg p-5 th-settings-panel"
        >
          <div className="flex justify-between items-center mb-4">
            <h3 className="font-bold text-lg flex items-center gap-2">
              <Settings className="w-5 h-5 text-primary" />
              {t('settings.title')}
            </h3>
            <button
              onClick={() => setIsSettingsOpen(false)}
              className="btn btn-ghost btn-sm btn-circle"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          <div role="tablist" className="tabs tabs-border mb-5">
            <button
              role="tab"
              className={`tab ${activeSettingsTab === 'display' ? 'tab-active' : ''}`}
              onClick={() => setActiveSettingsTab('display')}
            >
              {t('settings.tabDisplay')}
            </button>
            <button
              role="tab"
              className={`tab ${activeSettingsTab === 'behavior' ? 'tab-active' : ''}`}
              onClick={() => setActiveSettingsTab('behavior')}
            >
              {t('settings.tabBehavior')}
            </button>
            <button
              role="tab"
              className={`tab ${activeSettingsTab === 'intelligence' ? 'tab-active' : ''}`}
              onClick={() => setActiveSettingsTab('intelligence')}
            >
              {t('settings.tabIntelligence')}
            </button>
          </div>

          {activeSettingsTab === 'display' && (
            <div className="space-y-4">
              <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
                <Monitor className="w-4 h-4" /> {t('settings.displayHeading')}
              </h4>

              <div>
                <label className="label cursor-pointer">
                  <span>{t('settings.verticalText')}</span>
                  <input
                    type="checkbox"
                    className="toggle toggle-primary toggle-sm"
                    checked={vertical}
                    onChange={(e) => void updateSettings({ texthooker_vertical: e.target.checked })}
                  />
                </label>
              </div>

              <div>
                <label className="label cursor-pointer">
                  <span>{t('settings.alwaysVerticalBar')}</span>
                  <input
                    type="checkbox"
                    className="toggle toggle-primary toggle-sm"
                    checked={forceTopbarVertical}
                    onChange={(e) =>
                      void updateSettings({ texthooker_force_topbar_vertical: e.target.checked })
                    }
                  />
                </label>
              </div>

              <div className="rounded-lg border border-base-content/10 bg-base-200/40 p-3">
                <div className="text-[11px] font-semibold uppercase tracking-wide opacity-70 mb-2">
                  {t('settings.topBarMetrics')}
                </div>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('topbar.timer')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_show_timer ?? true}
                      onChange={(e) =>
                        void updateSettings({ texthooker_show_timer: e.target.checked })
                      }
                    />
                  </label>
                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('stats.speed')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_show_speed ?? true}
                      onChange={(e) =>
                        void updateSettings({ texthooker_show_speed: e.target.checked })
                      }
                    />
                  </label>
                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('stats.characters')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_show_characters ?? true}
                      onChange={(e) =>
                        void updateSettings({ texthooker_show_characters: e.target.checked })
                      }
                    />
                  </label>
                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('stats.lines')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_show_lines ?? true}
                      onChange={(e) =>
                        void updateSettings({ texthooker_show_lines: e.target.checked })
                      }
                    />
                  </label>
                </div>
              </div>

              <Field label={t('settings.fontSize')}>
                <input
                  type="number"
                  min="12"
                  max="72"
                  value={fontSize}
                  onChange={(e) =>
                    void updateSettings({ texthooker_font_size: Number(e.target.value) })
                  }
                  className="input input-sm w-full"
                />
              </Field>

              <Field label={t('settings.fontFamily')}>
                <select
                  value={fontFamily}
                  onChange={(e) => void updateSettings({ texthooker_font_family: e.target.value })}
                  className="select select-sm w-full"
                >
                  {FONT_FAMILY_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </Field>

              <Field label={t('settings.theme')} hint={t('settings.themeHint')}>
                <select
                  value={hookerTheme}
                  onChange={(e) => void updateSettings({ texthooker_theme: e.target.value })}
                  className="select select-sm w-full"
                >
                  {HOOKER_THEME_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.labelKey ? t(option.labelKey) : option.label}
                    </option>
                  ))}
                </select>
              </Field>

              <div className="grid grid-cols-2 gap-2">
                <Field label={t('settings.lineHeight')}>
                  <input
                    type="number"
                    min={0.5}
                    max={4}
                    step={0.1}
                    value={lineSpacing}
                    onChange={(e) =>
                      void updateSettings({ texthooker_line_spacing: Number(e.target.value) })
                    }
                    className="input input-sm w-full"
                  />
                </Field>
                <Field label={t('settings.lineGap')}>
                  <input
                    type="number"
                    min={0}
                    max={96}
                    step={1}
                    value={lineGap}
                    onChange={(e) =>
                      void updateSettings({ texthooker_line_gap: Number(e.target.value) })
                    }
                    className="input input-sm w-full"
                  />
                </Field>
              </div>

              <div className="rounded-lg border border-base-300 bg-base-200/40 p-4 mt-2">
                <div className="flex items-center justify-between mb-2">
                  <h4 className="font-semibold">{t('settings.customCss')}</h4>
                  <button
                    type="button"
                    className="btn btn-ghost btn-xs"
                    title={t('settings.classHelp')}
                    onClick={() => setIsClassHelpOpen(true)}
                  >
                    <HelpCircle className="w-4 h-4" />
                  </button>
                </div>
                <p className="text-sm text-base-content/70 mb-3">
                  {t('settings.customCssHint')}
                </p>
                <textarea
                  className="textarea w-full h-56 font-mono text-xs"
                  value={customCss}
                  onChange={(e) => void updateSettings({ texthooker_custom_css: e.target.value })}
                  placeholder={
                    '.th-line-text {\n  letter-spacing: 0.02em;\n}\n\n.th-topbar {\n  border-radius: 12px;\n}'
                  }
                />
                <div className="flex justify-end mt-3">
                  <button
                    type="button"
                    className="btn btn-outline btn-sm"
                    onClick={() => void updateSettings({ texthooker_custom_css: '' })}
                  >
                    {t('settings.clearCustomCss')}
                  </button>
                </div>
              </div>
            </div>
          )}

          {activeSettingsTab === 'behavior' && (
            <div className="space-y-4">
              <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
                <Activity className="w-4 h-4" /> {t('settings.behaviorHeading')}
              </h4>

              <Field
                label={t('settings.autoPause')}
                hint={t('settings.autoPauseHint')}
              >
                <input
                  type="number"
                  min={0}
                  max={3600}
                  value={autoPauseTimeout}
                  onChange={(e) =>
                    void updateSettings({ texthooker_auto_pause_timeout: Number(e.target.value) })
                  }
                  className="input input-sm w-full"
                />
              </Field>

              <Field
                label={t('settings.dictionaryPopupDelay')}
                hint={t('settings.dictionaryPopupDelayHint')}
              >
                <input
                  type="number"
                  min={0}
                  max={2000}
                  step={25}
                  value={dictionaryPopupDelay}
                  onChange={(event) =>
                    void updateSettings({
                      dictionary_popup_delay_ms: Math.max(0, Math.min(2000, Number(event.target.value) || 0)),
                    })
                  }
                  className="input input-sm w-full"
                />
              </Field>

              <div className="rounded-lg border border-base-content/10 bg-base-200/40 p-3">
                <div className="text-[11px] font-semibold uppercase tracking-wide opacity-70 mb-2">
                  {t('settings.runtime')}
                </div>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.clipboard')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_clipboard ?? false}
                      onChange={(e) =>
                        void updateSettings({ texthooker_clipboard: e.target.checked })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.allowLine')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_allow_line_during_pause ?? true}
                      onChange={(e) =>
                        void updateSettings({
                          texthooker_allow_line_during_pause: e.target.checked,
                        })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.allowClipboard')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_allow_clipboard_during_pause ?? true}
                      onChange={(e) =>
                        void updateSettings({
                          texthooker_allow_clipboard_during_pause: e.target.checked,
                        })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.autostartByLine')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_autostart_by_line ?? false}
                      onChange={(e) =>
                        void updateSettings({ texthooker_autostart_by_line: e.target.checked })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.autostartByClipboard')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_autostart_by_clipboard ?? false}
                      onChange={(e) =>
                        void updateSettings({ texthooker_autostart_by_clipboard: e.target.checked })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.pauseOnDisconnect')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_pause_on_disconnect ?? false}
                      onChange={(e) =>
                        void updateSettings({ texthooker_pause_on_disconnect: e.target.checked })
                      }
                    />
                  </label>

                  <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                    <span className="text-sm">{t('settings.autoconnect')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_autoconnect ?? true}
                      onChange={(e) =>
                        void updateSettings({ texthooker_autoconnect: e.target.checked })
                      }
                    />
                  </label>
                </div>
              </div>

              <Field label={t('settings.websocketUrl')}>
                <input
                  type="text"
                  value={settings?.texthooker_ws_url ?? ''}
                  onChange={(e) => void updateSettings({ texthooker_ws_url: e.target.value })}
                  className="input input-sm w-full"
                  placeholder="ws://localhost:6677"
                />
              </Field>

              <NativeHookTextFilters />

              <Field label={t('room.title')} error={roomError}>
                {(id) => (
                  <select
                    id={id}
                    className="select select-sm w-full"
                    value={roomMode}
                    onChange={(e) => {
                      disconnectRoom();
                      void updateSettings({ texthooker_room_mode: e.target.value });
                    }}
                  >
                    <option value="local">{t('room.local')}</option>
                    <option value="host">{t('room.host')}</option>
                    <option value="guest">{t('room.guest')}</option>
                  </select>
                )}
              </Field>

              {roomMode !== 'local' ? (
                <Field label={t('room.roomId')}>
                  {(id) => (
                    <div className="flex gap-2">
                      <input
                        id={id}
                        type="text"
                        className="input input-sm w-full font-mono"
                        value={roomId}
                        disabled={roomConnected}
                        placeholder={t('room.roomPlaceholder')}
                        onChange={(e) =>
                          void updateSettings({ texthooker_room_id: e.target.value })
                        }
                      />
                      {roomMode === 'host' && !roomId ? (
                        <button
                          type="button"
                          className="btn btn-sm btn-ghost"
                          onClick={() =>
                            void updateSettings({ texthooker_room_id: randomRoomId() })
                          }
                        >
                          {t('room.generate')}
                        </button>
                      ) : null}
                      <button
                        type="button"
                        className={`btn btn-sm ${roomConnected ? 'btn-error' : 'btn-primary'}`}
                        disabled={!roomId.trim()}
                        onClick={() => (roomConnected ? disconnectRoom() : connectRoom())}
                      >
                        {roomConnected
                          ? t('room.disconnect')
                          : roomMode === 'host'
                            ? t('room.hostAction')
                            : t('room.join')}
                      </button>
                    </div>
                  )}
                </Field>
              ) : null}

              {roomMode === 'host' && inviteLink ? (
                <Field label={t('room.inviteLink')} hint={t('room.inviteHint')}>
                  {(id) => (
                    <div className="flex gap-2">
                      <input
                        id={id}
                        type="text"
                        value={inviteLink}
                        readOnly
                        className="input input-sm w-full"
                      />
                      <button
                        type="button"
                        className={`btn btn-sm ${inviteCopied ? 'btn-success' : 'btn-outline'}`}
                        onClick={copyInvite}
                      >
                        {inviteCopied ? (
                          t('room.copied')
                        ) : (
                          <span className="inline-flex items-center gap-1">
                            <Share2 className="w-3.5 h-3.5" /> {t('room.copy')}
                          </span>
                        )}
                      </button>
                    </div>
                  )}
                </Field>
              ) : null}

              <div className="divider"></div>

              <div className="flex flex-col gap-2">
                <button
                  type="button"
                  onClick={() => void handleCopyAll()}
                  disabled={lines.length === 0}
                  className="btn btn-outline btn-sm"
                >
                  <Copy className="w-4 h-4" /> {t('settings.copyAllLines')}
                </button>
                <button
                  type="button"
                  onClick={() => { setClearError(false); setClearTarget(contentId); }}
                  disabled={lines.length === 0 || clearingLines}
                  className="btn btn-error btn-outline btn-sm"
                >
                  <Trash2 className="w-4 h-4" /> {t('settings.clearAllLines')}
                </button>
              </div>
            </div>
          )}
          {activeSettingsTab === 'intelligence' && (
            <div className="space-y-4">
              <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
                <Brain className="w-4 h-4 text-primary" /> {t('settings.intelligence.title')}
              </h4>
              <p className="text-sm text-base-content/70">{t('settings.intelligence.hint')}</p>
              <Field label={t('settings.intelligence.mode')}>
                <select
                  className="select select-sm w-full"
                  value={settings?.texthooker_intelligence_mode ?? 'automatic'}
                  onChange={(event) =>
                    void updateSettings({
                      texthooker_intelligence_mode: event.target.value as 'automatic' | 'manual' | 'off',
                    })
                  }
                >
                  <option value="automatic">{t('settings.intelligence.automatic')}</option>
                  <option value="manual">{t('settings.intelligence.manual')}</option>
                  <option value="off">{t('settings.intelligence.off')}</option>
                </select>
              </Field>
              {settings?.texthooker_intelligence_mode === 'manual' && (
                <Field label={t('settings.intelligence.manualThreshold')}>
                  <input
                    type="number"
                    min={1}
                    max={3600}
                    className="input input-sm w-full"
                    value={settings.texthooker_intelligence_manual_threshold_seconds ?? 40}
                    onChange={(event) =>
                      void updateSettings({
                        texthooker_intelligence_manual_threshold_seconds: Math.max(
                          1,
                          Math.min(3600, Number(event.target.value) || 1)
                        ),
                      })
                    }
                  />
                </Field>
              )}
              <Field label={t('settings.intelligence.afkThreshold')}>
                <input
                  type="number"
                  min={1}
                  max={7200}
                  className="input input-sm w-full"
                  value={settings?.texthooker_intelligence_afk_threshold_seconds ?? 300}
                  onChange={(event) =>
                    void updateSettings({
                      texthooker_intelligence_afk_threshold_seconds: Math.max(
                        1,
                        Math.min(7200, Number(event.target.value) || 1)
                      ),
                    })
                  }
                />
              </Field>
            </div>
          )}
        </div>
      )}

      <dialog className={`modal modal-bottom sm:modal-middle ${isClassHelpOpen ? 'modal-open' : ''}`}>
        <div className="modal-box max-w-2xl">
          <div className="flex justify-between items-center mb-4">
            <h3 className="font-bold text-lg">{t('classHelp.title')}</h3>
            <button
              onClick={() => setIsClassHelpOpen(false)}
              className="btn btn-ghost btn-sm btn-circle"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
          <p className="text-sm text-base-content/70 mb-3">
            {t('classHelp.intro')}
          </p>
          <div className="space-y-2 text-sm">
            <div className="surface-muted px-3 py-2">
              <code>.th-root</code> - {t('classHelp.root')}
            </div>
            <div className="surface-muted px-3 py-2">
              <code>.th-topbar</code> - {t('classHelp.topbar')}
            </div>
            <div className="surface-muted px-3 py-2">
              <code>.th-settings-panel</code> - {t('classHelp.settingsPanel')}
            </div>
            <div className="surface-muted px-3 py-2">
              <code>.th-line-list</code> - {t('classHelp.lineList')}
            </div>
            <div className="surface-muted px-3 py-2">
              <code>.th-line-item</code> - {t('classHelp.lineItem')}
            </div>
            <div className="surface-muted px-3 py-2">
              <code>.th-line-text</code> - {t('classHelp.lineText')}
            </div>
          </div>
          <div className="modal-action">
            <button className="btn" onClick={() => setIsClassHelpOpen(false)}>
              {t('common:close')}
            </button>
          </div>
        </div>
        <form method="dialog" className="modal-backdrop">
          <button onClick={() => setIsClassHelpOpen(false)}>close</button>
        </form>
      </dialog>


      <LineList
        lines={lines}
        contentId={contentId}
        vertical={vertical}
        lineGap={lineGap}
        lineSpacing={lineSpacing}
        fontSize={fontSize}
        fontFamily={fontFamily}
        saveLabel={t('mining.saveLine')}
        deleteLabel={t('deleteLine')}
        onPointer={openDictionaryAtPointer}
        onLeave={cancelDictionaryHover}
        onSave={saveLine}
        onDelete={deleteLine}
      />

      {dictionaryRequest ? (
        <DictionaryPopup
          initial={dictionaryRequest}
          miningContext={dictionaryContext}
          entries={dictionaryEntries}
          loading={dictionaryLoading}
          anchor={dictionaryAnchor}
          vertical={dictionaryVertical}
          theme={effectiveHookerTheme}
          onLookup={lookupDictionary}
          onClose={closeDictionary}
        />
      ) : null}

      <SessionPickerModal
        open={isSessionPickerOpen}
        currentId={contentId}
        onClose={() => setIsSessionPickerOpen(false)}
        onPick={async (session) => {
          await ipc.setHookerSession(
            session.contentId,
            session.mediaTitle ?? undefined,
            session.mediaType ?? undefined
          );
          setIsSessionPickerOpen(false);
          void queryClient.invalidateQueries({ queryKey: ['text-sessions'] });
          await reload();
        }}
        onSearch={() => {
          setIsSessionPickerOpen(false);
          setAttaching(true);
        }}
      />

      <Modal
        open={clearTarget === contentId}
        onClose={() => { if (!clearingLines) setClearTarget(null); }}
        title={t('settings.clearConfirmTitle')}
        dismissable={!clearingLines}
        actions={<>
          <Button appearance="ghost" disabled={clearingLines} onClick={() => setClearTarget(null)}>{t('common:cancel')}</Button>
          <Button variant="error" loading={clearingLines} onClick={() => void handleClearAll()}>{t('settings.clearAllLines')}</Button>
        </>}
      >
        <p>{t('settings.clearConfirmBody')}</p>
        {clearError && <p role="alert" className="mt-3 text-sm text-error">{t('settings.clearFailed')}</p>}
      </Modal>

      <Modal
        open={isHookPickerOpen}
        onClose={() => setIsHookPickerOpen(false)}
        title={t('hookSelection.title')}
        size="lg"
        actions={(
          <>
            <Button appearance="ghost" onClick={() => setIsHookPickerOpen(false)}>
              {t('common:cancel')}
            </Button>
            <Button
              onClick={() => void saveNativeHookSelection()}
              disabled={
                !selectedHookCode
                || !nativeHook?.hooks.some((hook) => hook.selection_key === selectedHookCode)
                || savingHook
              }
            >
              {savingHook ? t('hookSelection.saving') : t('hookSelection.save')}
            </Button>
          </>
        )}
      >
        <div className="max-h-[55vh] overflow-y-auto">
          <NativeHookChoices status={nativeHook} title={detectedVn?.media_title} selected={selectedHookCode} onSelect={setSelectedHookCode}
            enable={!nativeHook && detectedVn && !hookApp?.native_hook_enabled ? { busy: enablingHook, disabled: !hookApp || hookApps.isPending, onClick: () => void enableNativeHook() } : undefined}
            trackingRequired={!nativeHook && Boolean(hookApp?.native_hook_enabled)} />
        </div>
        {hookSaveError && <p className="mt-3 text-sm text-error">{hookSaveError}</p>}
      </Modal>

      <MediaSearchModal
        open={attaching}
        onClose={() => setAttaching(false)}
        mediaType={totals?.mediaType ?? 'vn'}
        initialQuery={hooker?.content_title ?? ''}
        onPick={async (hit, type) => {
          await ipc.hookerAttachMedia(
            hit.contentId,
            hit.title.contentTitleRomaji || hit.title.contentTitleNative,
            hit.type ?? type
          );
          setAttaching(false);
          void queryClient.invalidateQueries({ queryKey: ['text-sessions'] });
          await reload();
        }}
      />
    </div>
  );
}

/** Every session stored locally, plus the free-form one. */
function SessionPickerModal({
  open,
  currentId,
  onClose,
  onPick,
  onSearch,
}: {
  open: boolean;
  currentId: string;
  onClose: () => void;
  onPick: (session: TextSessionSummary) => void | Promise<void>;
  onSearch: () => void;
}) {
  const { t } = useTranslation('texthooker');
  const { data } = useQuery({
    queryKey: ['text-sessions'],
    queryFn: ipc.listTextSessions,
    enabled: open,
  });

  const rows = useMemo(() => {
    const sessions = data ?? [];
    if (sessions.some((s) => s.contentId === LOCAL_SESSION)) return sessions;
    return [
      {
        contentId: LOCAL_SESSION,
        mediaTitle: null,
        mediaType: null,
        timerSeconds: 0,
        lines: 0,
        chars: 0,
        sessionChars: 0,
        sessionLines: 0,
      },
      ...sessions,
    ];
  }, [data]);

  return (
    <dialog className={`modal modal-bottom sm:modal-middle ${open ? 'modal-open' : ''}`}>
      <div className="modal-box max-w-2xl">
        <div className="flex justify-between items-center mb-4">
          <h3 className="font-bold text-lg">{t('sessions.title')}</h3>
          <button onClick={onClose} className="btn btn-ghost btn-sm btn-circle">
            <X className="w-5 h-5" />
          </button>
        </div>
        <ul className="max-h-96 overflow-y-auto space-y-1">
          {rows.map((session) => (
            <li key={session.contentId}>
              <button
                type="button"
                className={`flex w-full items-center gap-3 rounded-field px-3 py-2 text-left transition-colors cursor-pointer ${
                  session.contentId === currentId
                    ? 'bg-primary text-primary-content'
                    : 'hover:bg-base-200'
                }`}
                onClick={() => void onPick(session)}
              >
                <span className="min-w-0 flex-1">
                  <span className="block font-medium truncate">
                    {session.contentId === LOCAL_SESSION
                      ? t('sessions.free')
                      : (session.mediaTitle ?? session.contentId)}
                  </span>
                  <span className="block text-xs opacity-70 font-mono">{session.contentId}</span>
                </span>
                <span className="text-xs tabular-nums opacity-80 shrink-0">
                  {numberWithCommas(session.chars)} car. · {formatClock(session.timerSeconds)}
                </span>
              </button>
            </li>
          ))}
        </ul>
        <div className="modal-action">
          <button type="button" className="btn btn-primary" onClick={onSearch}>
            {t('sessions.linkAnother')}
          </button>
        </div>
      </div>
      <form method="dialog" className="modal-backdrop">
        <button onClick={onClose}>close</button>
      </form>
    </dialog>
  );
}
