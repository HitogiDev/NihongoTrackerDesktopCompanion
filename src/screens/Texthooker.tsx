import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import {
  Inbox,
  X,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { Modal } from '../components/ui/Modal';
import { DictionaryPopup } from '../components/DictionaryPopup';
import { NativeHookChoices } from '../components/NativeHookChoices';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { numberWithCommas } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { miningError } from '../lib/mining';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from '../store/useAppStore';
import { useManualTimer } from './texthooker/useManualTimer';
import { useHookerLines } from './texthooker/useHookerLines';
import { useRoom } from './texthooker/useRoom';
import { useDictionaryHover } from '../hooks/useDictionaryHover';
import { LineList } from './texthooker/LineList';
import { TopBar } from './texthooker/TopBar';
import { TimerEditModal, TimerResetModal } from './texthooker/TimerEditModal';
import { StatsModal } from './texthooker/StatsModal';
import { SettingsPanel, type SettingsTab } from './texthooker/SettingsPanel';
import { LOCAL_SESSION } from './texthooker/constants';
import { SessionPickerModal } from './texthooker/SessionPickerModal';
import { ModalBackdrop } from '../components/ui/Modal';
import { toast } from '../store/useToastStore';

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
  const hooker = useAppStore((s) => s.hooker);

  const contentId = hooker?.content_id ?? LOCAL_SESSION;
  const [endedSession, setEndedSession] = useState<EndedSession | null>(null);

  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>('display');
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
      toast.success(t('mining.saved'));
    } catch (error) {
      toast.error(miningError(error));
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

      {isSettingsOpen && (
        <SettingsPanel
          settingsButtonRef={settingsButtonRef}
          activeSettingsTab={activeSettingsTab}
          setActiveSettingsTab={setActiveSettingsTab}
          onClose={() => setIsSettingsOpen(false)}
          onOpenClassHelp={() => setIsClassHelpOpen(true)}
          roomMode={roomMode}
          roomId={roomId}
          roomConnected={roomConnected}
          roomError={roomError}
          inviteLink={inviteLink}
          inviteCopied={inviteCopied}
          copyInvite={copyInvite}
          connectRoom={connectRoom}
          disconnectRoom={disconnectRoom}
          hasLines={lines.length > 0}
          clearingLines={clearingLines}
          onCopyAll={() => void handleCopyAll()}
          onClearAll={() => { setClearError(false); setClearTarget(contentId); }}
        />
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
        <ModalBackdrop onClose={() => setIsClassHelpOpen(false)} />
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
