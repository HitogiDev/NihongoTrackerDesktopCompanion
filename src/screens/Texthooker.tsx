import { NativeHookTextFilters } from '../components/NativeHookTextFilters';
import { useReadingOverlay } from '../hooks/useReadingOverlay';
import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Activity,
  BarChart2,
  BookOpen,
  BookmarkPlus,
  Brain,
  ChevronLeft,
  ChevronRight,
  Clock,
  Copy,
  Crown,
  Edit3,
  FileText,
  HelpCircle,
  Home,
  Link as LinkIcon,
  Monitor,
  Pause,
  PanelTop,
  Play,
  RotateCcw,
  Save,
  SlidersHorizontal,
  Target,
  Settings,
  Trash2,
  Share2,
  Type,
  Undo2,
  Unlink,
  Users,
  X,
} from 'lucide-react';
import { Field } from '../components/ui/Field';
import { Button } from '../components/ui/Button';
import { Modal } from '../components/ui/Modal';
import { DictionaryPopup } from '../components/DictionaryPopup';
import { textHitAtPoint } from '../lib/textHit';
import { NativeHookChoices } from '../components/NativeHookChoices';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { formatClock, numberWithCommas, readingSpeed } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { miningError } from '../lib/mining';
import { useAppStore } from '../store/useAppStore';
import {
  RoomConnection,
  randomRoomId,
  type RoomLine,
  type RoomMember,
} from '../lib/room';
import type {
  DictionaryEntry,
  HookerTotals,
  TextLine,
  SessionIntelligence,
  TextSessionSummary,
} from '../lib/types';
import type { LookupRequest } from '@nihongotracker/yomitan-content';

const LOCAL_SESSION = 'local';

// Same options and stacks as NihongoTracker/Frontend/src/screens/HookerScreen.tsx.
const HOOKER_THEME_OPTIONS: Array<{ value: string; label?: string; labelKey?: string }> = [
  { value: '', labelKey: 'settings.useAppTheme' },
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'cupcake', label: 'Cupcake' },
  { value: 'bumblebee', label: 'Bumblebee' },
  { value: 'emerald', label: 'Emerald' },
  { value: 'corporate', label: 'Corporate' },
  { value: 'synthwave', label: 'Synthwave' },
  { value: 'retro', label: 'Retro' },
  { value: 'valentine', label: 'Valentine' },
  { value: 'halloween', label: 'Halloween' },
  { value: 'garden', label: 'Garden' },
  { value: 'forest', label: 'Forest' },
  { value: 'dracula', label: 'Dracula' },
  { value: 'cmyk', label: 'CMYK' },
  { value: 'night', label: 'Night' },
  { value: 'winter', label: 'Winter' },
  { value: 'dim', label: 'Dim' },
  { value: 'sunset', label: 'Sunset' },
  { value: 'abyss', label: 'Abyss' },
];

function analyzeSession(
  lines: TextLine[],
  mode: SessionIntelligence['detectionMode'],
  manualThreshold: number,
  afkThreshold: number
): SessionIntelligence {
  const japanese = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/gu;
  const chronological = lines
    .map((line) => ({ ...line, timestamp: Date.parse(line.created_at) }))
    .filter((line) => Number.isFinite(line.timestamp))
    .sort((a, b) => a.timestamp - b.timestamp);
  const gaps = chronological.slice(1).map((line, index) =>
    Math.max(0, Math.round((line.timestamp - chronological[index].timestamp) / 1000))
  );
  const baselineGaps = gaps.filter((gap) => gap > 0 && gap < afkThreshold).sort((a, b) => a - b);
  const baseline = baselineGaps.length ? baselineGaps[Math.floor(baselineGaps.length / 2)] : null;
  const threshold = mode === 'manual' ? manualThreshold : mode === 'automatic' && baseline !== null ? Math.max(40, baseline * 2) : null;
  let focusedSeconds = 0;
  let distractedSeconds = 0;
  let afkSeconds = 0;
  let distractionCount = 0;
  let longestDistractionSeconds = 0;
  gaps.forEach((gap) => {
    if (mode === 'off') return;
    if (gap >= afkThreshold) afkSeconds += gap;
    else if (threshold !== null && gap > threshold) {
      const distraction = gap - threshold;
      focusedSeconds += threshold;
      distractedSeconds += distraction;
      distractionCount += 1;
      longestDistractionSeconds = Math.max(longestDistractionSeconds, distraction);
    } else focusedSeconds += gap;
  });
  const firstTimestamp = chronological[0]?.timestamp ?? 0;
  const charactersPerMinute: number[] = [];
  chronological.forEach((line) => {
    const minute = Math.min(1439, Math.floor(Math.max(0, line.timestamp - firstTimestamp) / 60_000));
    while (charactersPerMinute.length <= minute) charactersPerMinute.push(0);
    charactersPerMinute[minute] += (line.text.match(japanese) ?? []).length;
  });
  const attentionSeconds = focusedSeconds + distractedSeconds;
  return {
    detectionMode: mode,
    baselineIntervalSeconds: baseline,
    distractionThresholdSeconds: threshold,
    afkThresholdSeconds: afkThreshold,
    focusedSeconds,
    distractedSeconds,
    afkSeconds,
    distractionCount,
    longestDistractionSeconds,
    focusPercentage: attentionSeconds ? Math.round((focusedSeconds / attentionSeconds) * 100) : 0,
    charactersPerMinute,
  };
}

function durationLabel(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.round(seconds % 60);
  return minutes ? `${minutes}m ${remainder}s` : `${remainder}s`;
}

const FONT_FAMILY_OPTIONS = [
  { value: 'Noto Sans JP', label: 'Noto Sans JP' },
  { value: 'Noto Serif JP', label: 'Noto Serif JP' },
  { value: 'Sawarabi Gothic', label: 'Sawarabi Gothic' },
  { value: 'Ackaisyo', label: 'Ackaisyo' },
  { value: 'Hyoukinna Pop', label: 'Hyoukinna Pop' },
  { value: 'Humour Original', label: 'Humour Original' },
  { value: 'sans-serif', label: 'Sans Serif' },
];

const FONT_FAMILY_STACKS: Record<string, string> = {
  'Noto Sans JP': '"Noto Sans JP Local", "Noto Sans JP", "Noto Sans CJK JP", sans-serif',
  'Noto Serif JP': '"Noto Serif JP Local", "Noto Serif JP", serif',
  'Sawarabi Gothic': '"Sawarabi Gothic", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  Ackaisyo: '"Ackaisyo", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'Hyoukinna Pop': '"Hyoukinna Pop", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'Humour Original': '"Humour Original", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'sans-serif': 'sans-serif',
};

const COLUMN_HEIGHT = 'calc(100vh - 160px)';

export default function TexthookerScreen() {
  const { t } = useTranslation(['texthooker', 'common']);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const hooker = useAppStore((s) => s.hooker);
  const tracker = useAppStore((s) => s.tracker);
  const readingOverlay = useReadingOverlay();
  const online = useAppStore((s) => s.online);

  const contentId = hooker?.content_id ?? LOCAL_SESSION;
  const contentIdRef = useRef(contentId);
  contentIdRef.current = contentId;
  const reloadRequestRef = useRef(0);
  const displayedContentIdRef = useRef(contentId);

  const [lines, setLines] = useState<TextLine[]>([]);
  const [totals, setTotals] = useState<HookerTotals | null>(null);
  const [undoStack, setUndoStack] = useState<TextLine[][]>([]);
  const [miningFeedback, setMiningFeedback] = useState<string | null>(null);

  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<'display' | 'behavior' | 'intelligence'>('display');
  const [isStatsOpen, setIsStatsOpen] = useState(false);
  const [expandedIntelligenceId, setExpandedIntelligenceId] = useState<string | null>(null);
  const [isTimerEditOpen, setIsTimerEditOpen] = useState(false);
  const [isResetTimerConfirmOpen, setIsResetTimerConfirmOpen] = useState(false);
  const [savingTimer, setSavingTimer] = useState(false);
  const [timerError, setTimerError] = useState(false);
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
  const [isLogging, setIsLogging] = useState(false);
  const [logComment, setLogComment] = useState('');
  const [isLogAnimating, setIsLogAnimating] = useState(false);
  const [dictionaryRequest, setDictionaryRequest] = useState<LookupRequest | null>(null);
  const [dictionaryAnchor, setDictionaryAnchor] = useState<DOMRect | null>(null);
  const [dictionaryVertical, setDictionaryVertical] = useState(false);
  const [dictionaryEntries, setDictionaryEntries] = useState<DictionaryEntry[] | undefined>();
  const [dictionaryLoading, setDictionaryLoading] = useState(false);
  const dictionaryHoverTimerRef = useRef<number | null>(null);
  const dictionaryHoverKeyRef = useRef<string | null>(null);
  const dictionaryPointerRef = useRef<{ element: HTMLElement; text: string; x: number; y: number; context?: { lineId: string; contentId: string } } | null>(null);

  const [timerEditHours, setTimerEditHours] = useState(0);
  const [timerEditMinutes, setTimerEditMinutes] = useState(0);
  const [timerEditSeconds, setTimerEditSeconds] = useState(0);

  // ── shared room ─────────────────────────────────────────────────────
  const [roomConnected, setRoomConnected] = useState(false);
  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [roomError, setRoomError] = useState<string | null>(null);
  const [inviteCopied, setInviteCopied] = useState(false);
  const roomRef = useRef<RoomConnection | null>(null);

  const [isTimerActive, setIsTimerActive] = useState(false);
  const [manualSeconds, setManualSeconds] = useState(0);
  const anchorRef = useRef({ at: Date.now(), seconds: 0 });
  const secondsRef = useRef(0);
  const lastActivityRef = useRef(Date.now());

  const listContainerRef = useRef<HTMLDivElement>(null);
  const followLatestRef = useRef(true);
  const lastScrollExtentRef = useRef<number | null>(null);
  const topbarRef = useRef<HTMLDivElement>(null);
  const settingsPanelRef = useRef<HTMLDivElement>(null);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const topbarNaturalWidthRef = useRef(0);
  const isTopbarVerticalRef = useRef(false);
  const [isTopbarVertical, setIsTopbarVertical] = useState(false);

  // A detected visual novel drives the clock from Rust; the manual timer is for
  // everything else.
  const autoSession = tracker?.vn && tracker.vn.content_id === contentId ? tracker.vn : null;
  const nativeHook = tracker?.game_hook.exe ? tracker.game_hook : null;
  const nativeSession = nativeHook?.content_id === contentId;
  const nativeSelected = Boolean(nativeSession && !nativeHook.websocket_selected);
  const nativeInput = nativeSelected && nativeHook
    && ['starting', 'attached', 'receiving'].includes(nativeHook.phase);
  const hookApps = useQuery({ queryKey: ['vn-apps'], queryFn: ipc.listVnApps, enabled: isHookPickerOpen });
  const hookApp = hookApps.data?.find((entry) => entry.exe.toLowerCase() === tracker?.vn?.exe.toLowerCase());
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
  const seconds = autoSession ? autoSession.elapsed_secs : manualSeconds;

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
  const isTopbarHidden = settings?.texthooker_topbar_hidden ?? false;
  const forceTopbarVertical = settings?.texthooker_force_topbar_vertical ?? false;
  const autoPauseTimeout = settings?.texthooker_auto_pause_timeout ?? 120;
  const dictionaryPopupDelay = Math.max(0, Math.min(2000, settings?.dictionary_popup_delay_ms ?? 75));
  const effectiveTopbarVertical = forceTopbarVertical || isTopbarVertical;
  const connectionStatus = hooker?.status ?? 'disconnected';
  const roomMode = (settings?.texthooker_room_mode ?? 'local') as 'local' | 'host' | 'guest';
  const roomId = settings?.texthooker_room_id ?? '';
  const inviteLink =
    roomId && settings?.api_base_url
      ? `${settings.api_base_url.replace(/\/+$/, '')}/texthooker/session?mode=guest&roomId=${roomId}`
      : '';

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

  const openDictionaryAtPointer = useCallback((event: ReactMouseEvent<HTMLElement>, line: TextLine) => {
    const element = event.currentTarget;
    const text = line.text;
    const context = { lineId: line.id, contentId: line.content_id };
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

  // ── data ────────────────────────────────────────────────────────────

  const reload = useCallback(async () => {
    if (contentIdRef.current !== contentId) return;
    const requestId = ++reloadRequestRef.current;
    const [nextLines, nextTotals] = await Promise.all([
      ipc.hookerLines(contentId, 5000),
      ipc.hookerTotals(contentId),
    ]);
    if (
      requestId !== reloadRequestRef.current ||
      contentIdRef.current !== contentId
    ) return;
    setLines(nextLines);
    setTotals(nextTotals);
    if (!autoSession) {
      setManualSeconds(nextTotals.timerSeconds);
      secondsRef.current = nextTotals.timerSeconds;
      anchorRef.current = { at: Date.now(), seconds: nextTotals.timerSeconds };
    }
  }, [contentId, autoSession]);

  useEffect(() => {
    if (displayedContentIdRef.current !== contentId) {
      displayedContentIdRef.current = contentId;
      setLines([]);
      setTotals(null);
      setIsTimerActive(false);
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
        void queryClient.invalidateQueries({ queryKey: ['text-context'] });
        lastActivityRef.current = Date.now();
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
  }, [contentId, queryClient]);

  // A line produced locally is mirrored to the room when hosting.
  useEffect(() => {
    const unlisten = listen<{ line: TextLine }>(events.hookerLine, (event) => {
      const room = roomRef.current;
      if (!room?.connected || roomMode !== 'host') return;
      if (event.payload.line.content_id !== contentId) return;
      room.sendLine({
        id: event.payload.line.id,
        text: event.payload.line.text,
        japaneseCount: event.payload.line.chars,
        createdAt: event.payload.line.created_at,
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [contentId, roomMode]);

  // Rust starts the clock when a line arrives and autostart is on.
  useEffect(() => {
    const unlisten = listen<{ active: boolean }>(events.hookerTimer, (event) => {
      setIsTimerActive(event.payload.active);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    void ipc.hookerSetTimerActive(isTimerActive);
  }, [isTimerActive]);

  // ── timer ───────────────────────────────────────────────────────────

  const anchorTimer = useCallback((value: number) => {
    anchorRef.current = { at: Date.now(), seconds: value };
  }, []);

  // Wall-clock anchored, exactly like the web client: a throttled render loop
  // must never lose time, and the auto-pause deadline caps how much of a gap
  // can be banked.
  useEffect(() => {
    if (autoSession || !isTimerActive) return;
    anchorTimer(secondsRef.current);

    const tick = () => {
      const now = Date.now();
      const pauseAt =
        autoPauseTimeout > 0 ? lastActivityRef.current + autoPauseTimeout * 1000 : Infinity;
      const countedUntil = Math.min(now, pauseAt);
      const { at, seconds: banked } = anchorRef.current;
      const next = banked + Math.max(0, Math.floor((countedUntil - at) / 1000));

      if (next !== secondsRef.current) {
        secondsRef.current = next;
        setManualSeconds(next);
      }
      if (now >= pauseAt) {
        anchorTimer(next);
        setIsTimerActive(false);
      }
    };

    const interval = setInterval(tick, 1000);
    document.addEventListener('visibilitychange', tick);
    return () => {
      clearInterval(interval);
      document.removeEventListener('visibilitychange', tick);
    };
  }, [isTimerActive, autoPauseTimeout, autoSession, anchorTimer]);

  // Persist the manual clock so a restart doesn't lose the sitting.
  useEffect(() => {
    if (autoSession) return;
    const id = setInterval(() => {
      void ipc.hookerSetTimer(contentId, secondsRef.current);
    }, 15000);
    return () => {
      clearInterval(id);
      void ipc.hookerSetTimer(contentId, secondsRef.current);
    };
  }, [contentId, autoSession]);

  // ── room ────────────────────────────────────────────────────────────

  const disconnectRoom = useCallback(() => {
    roomRef.current?.disconnect();
    roomRef.current = null;
    setRoomConnected(false);
    setRoomMembers([]);
  }, []);

  const connectRoom = useCallback(() => {
    if (!settings || roomMode === 'local' || !roomId.trim()) return;
    disconnectRoom();
    setRoomError(null);

    const room = new RoomConnection(
      {
        baseUrl: settings.api_base_url,
        roomId: roomId.trim(),
        role: roomMode,
        username: settings.username,
        // Same key the web client uses, so hosting the same room from either
        // client reclaims it instead of being rejected.
        hostToken: localStorage.getItem(`hostToken_${roomId.trim()}`),
      },
      {
        onStatus: setRoomConnected,
        onCreated: ({ roomId: id, hostToken }) =>
          localStorage.setItem(`hostToken_${id}`, hostToken),
        onJoined: () => setRoomError(null),
        onHistory: (history) => {
          void Promise.all(history.map((line) => ipc.hookerIngest(line.text))).then(reload);
        },
        onLine: (line: RoomLine) => {
          void ipc.hookerIngest(line.text);
        },
        onLinesDeleted: () => void reload(),
        onLinesRestored: () => void reload(),
        onMembers: setRoomMembers,
        onError: setRoomError,
      }
    );
    room.connect();
    roomRef.current = room;
  }, [settings, roomMode, roomId, disconnectRoom, reload]);

  // Leave the room when the screen goes away.
  useEffect(() => () => disconnectRoom(), [disconnectRoom]);

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
  }, [autoSession, anchorTimer]);

  const handleOpenTimerEdit = useCallback(() => {
    setTimerEditHours(Math.floor(seconds / 3600));
    setTimerEditMinutes(Math.floor((seconds % 3600) / 60));
    setTimerEditSeconds(seconds % 60);
    setTimerError(false);
    setIsTimerEditOpen(true);
  }, [seconds]);

  const handleSaveTimerEdit = useCallback(async () => {
    const total = timerEditHours * 3600 + timerEditMinutes * 60 + timerEditSeconds;
    setSavingTimer(true); setTimerError(false);
    try {
      await ipc.hookerSetTimer(contentId, total, true);
      if (contentIdRef.current !== contentId) return;
      secondsRef.current = total;
      setManualSeconds(total);
      anchorTimer(total);
      lastActivityRef.current = Date.now();
      setIsTimerEditOpen(false);
    } catch { setTimerError(true); }
    finally { setSavingTimer(false); }
  }, [timerEditHours, timerEditMinutes, timerEditSeconds, contentId, anchorTimer]);

  const handleResetTimer = useCallback(async () => {
    setSavingTimer(true); setTimerError(false);
    try {
      if (autoSession) await ipc.setSessionPaused(true);
      await ipc.hookerSetTimer(contentId, 0, true);
      if (contentIdRef.current !== contentId) return;
      secondsRef.current = 0;
      setManualSeconds(0);
      anchorTimer(0);
      setIsTimerActive(false);
      setIsResetTimerConfirmOpen(false);
    } catch { setTimerError(true); }
    finally { setSavingTimer(false); }
  }, [contentId, autoSession, anchorTimer]);

  useEffect(() => { setIsTimerEditOpen(false); setIsResetTimerConfirmOpen(false); setTimerError(false); }, [contentId]);

  // ── lines ───────────────────────────────────────────────────────────

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

  const handleClearAll = useCallback(async () => {
    if (lines.length === 0 || clearTarget !== contentId || clearingLines) return;
    setClearingLines(true);
    setClearError(false);
    try {
      await ipc.hookerClear(contentId);
      if (contentIdRef.current !== contentId) return;
      setUndoStack((prev) => [...prev, lines]);
      setClearTarget(null);
      await reload();
    } catch {
      if (contentIdRef.current === contentId) setClearError(true);
    } finally {
      setClearingLines(false);
    }
  }, [lines, contentId, clearTarget, clearingLines, reload]);

  useEffect(() => { setClearTarget(null); setClearError(false); }, [contentId]);

  const handleCopyAll = useCallback(async () => {
    const combined = lines.map((line) => line.text).join('\n');
    if (!combined) return;
    await navigator.clipboard.writeText(combined).catch(() => undefined);
  }, [lines]);

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
  const allTimeLines = Math.max(context?.lines ?? 0, linesNumber);
  const allTimeChars = Math.max(context?.characters ?? 0, charsNumber);
  const currentSessionChars = autoSession?.session_chars ?? totals?.sessionChars ?? 0;
  const currentSessionLines = autoSession?.session_lines ?? totals?.sessionLines ?? 0;
  const progressCharacters = (context?.loggedCharacters ?? 0) + currentSessionChars;
  const progressPercent = context?.totalCharacters
    ? Math.min(100, progressCharacters / context.totalCharacters * 100) : null;
  const history = context?.history;
  const cover = context?.cover ?? autoSession?.image;
  const sessionIntelligence = useMemo(
    () => analyzeSession(
      currentSessionLines > 0 ? lines.slice(-currentSessionLines) : [],
      settings?.texthooker_intelligence_mode ?? 'automatic',
      Math.max(1, settings?.texthooker_intelligence_manual_threshold_seconds ?? 40),
      Math.max(1, settings?.texthooker_intelligence_afk_threshold_seconds ?? 300)
    ),
    [currentSessionLines, lines, settings?.texthooker_intelligence_afk_threshold_seconds,
      settings?.texthooker_intelligence_manual_threshold_seconds, settings?.texthooker_intelligence_mode]
  );

  useLayoutEffect(() => {
    followLatestRef.current = true;
    lastScrollExtentRef.current = null;
  }, [contentId, vertical]);

  useLayoutEffect(() => {
    const el = listContainerRef.current;
    if (!el) return;
    if (followLatestRef.current) {
      if (vertical) el.scrollLeft = Math.min(0, el.clientWidth - el.scrollWidth);
      else el.scrollTop = el.scrollHeight;
    }
    lastScrollExtentRef.current = vertical ? el.scrollWidth : el.scrollHeight;
  }, [lines, vertical]);

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

  useEffect(() => {
    isTopbarVerticalRef.current = isTopbarVertical;
  }, [isTopbarVertical]);

  // Flip the bar to a column once it no longer fits. scrollWidth is only the
  // natural row width while it is actually a row, so keep the last known one.
  useEffect(() => {
    const checkLayout = () => {
      const el = topbarRef.current;
      if (el && !isTopbarVerticalRef.current) {
        topbarNaturalWidthRef.current = el.scrollWidth;
      }
      if (topbarNaturalWidthRef.current === 0) return;
      const shouldBeVertical = topbarNaturalWidthRef.current > window.innerWidth - 16;
      if (shouldBeVertical !== isTopbarVerticalRef.current) {
        setIsTopbarVertical(shouldBeVertical);
      }
    };
    checkLayout();
    window.addEventListener('resize', checkLayout);
    const interval = setInterval(checkLayout, 500);
    return () => {
      window.removeEventListener('resize', checkLayout);
      clearInterval(interval);
    };
  }, []);

  const listStyles: CSSProperties = vertical
    ? { columnGap: lineGap, rowGap: lineGap, direction: 'rtl' }
    : { rowGap: lineGap };

  // Memoized, like the handlers below, so `LineItem` can skip the lines that did
  // not change: this screen re-renders every second while a session counts.
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

  const topbarMetrics: Array<{ key: string; title: string; content: ReactNode }> = [];
  if (settings?.texthooker_show_timer) {
    topbarMetrics.push({
      key: 'timer',
      title: t('topbar.timer'),
      content: (
        <>
          <Clock className="w-4 h-4 opacity-70" />
          {formatClock(seconds)}
        </>
      ),
    });
  }
  if (settings?.texthooker_show_speed) {
    topbarMetrics.push({
      key: 'reading-speed',
      title: t('topbar.speed'),
      content: (
        <>
          <Activity className="w-4 h-4 opacity-70" />
          {numberWithCommas(readingSpeed(charsNumber, seconds))}/h
        </>
      ),
    });
  }
  if (settings?.texthooker_show_characters) {
    topbarMetrics.push({
      key: 'characters',
      title: t('topbar.characters'),
      content: (
        <>
          <Type className="w-4 h-4 opacity-70" />
          {numberWithCommas(charsNumber)}
        </>
      ),
    });
  }
  if (settings?.texthooker_show_lines) {
    topbarMetrics.push({
      key: 'lines',
      title: t('topbar.lines'),
      content: (
        <>
          <FileText className="w-4 h-4 opacity-70" />
          {numberWithCommas(linesNumber)}
        </>
      ),
    });
  }

  const listContainerClasses = vertical
    ? 'pt-20 px-4 flex flex-row overflow-x-auto overflow-y-hidden h-screen items-start pb-6'
    : 'pt-20 px-4 flex flex-col overflow-y-auto h-screen pb-6';

  const topbarIconBtnClass = effectiveTopbarVertical
    ? 'btn btn-xs btn-ghost w-full justify-center'
    : 'btn btn-xs btn-ghost btn-square';

  const systemTheme =
    typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: light)').matches
      ? 'light'
      : 'dark';
  const effectiveHookerTheme =
    hookerTheme === 'system' ? systemTheme : hookerTheme || undefined;

  async function handleLogSession() {
    if (!totals || currentSessionChars === 0) return;
    setIsLogging(true);
    try {
      await ipc.logTextSession({
        content_id: contentId,
        media_type: totals.mediaType ?? 'vn',
        description: totals.mediaTitle ?? hooker?.content_title ?? t('sessions.free'),
        comment: logComment.trim(),
        chars: currentSessionChars,
        seconds,
        intelligence: sessionIntelligence,
      });
      setIsLogAnimating(true);
      setLogComment('');
      setTimeout(() => setIsLogAnimating(false), 2000);
      secondsRef.current = 0;
      setManualSeconds(0);
      anchorTimer(0);
      setIsTimerActive(false);
      await reload();
      void queryClient.invalidateQueries({ queryKey: ['outbox'] });
      void queryClient.invalidateQueries({ queryKey: ['text-context'] });
    } finally {
      setIsLogging(false);
    }
  }

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
      {miningFeedback ? <button type="button" className="fixed bottom-3 right-3 z-50 alert alert-info w-auto max-w-sm text-sm" onClick={() => setMiningFeedback(null)}>{miningFeedback}</button> : null}

      {/* Top Bar */}
      {isTopbarHidden && effectiveTopbarVertical ? (
        <button
          type="button"
          onClick={() => void updateSettings({ texthooker_topbar_hidden: false })}
          className="fixed top-1/2 right-0 -translate-y-1/2 w-4 h-10 flex items-center justify-center bg-base-100 rounded-l-md shadow-sm z-50 text-base-content"
          title={t('topbar.show')}
        >
          <ChevronLeft className="w-3.5 h-3.5" />
        </button>
      ) : (
        <div
          ref={topbarRef}
          className={`fixed top-1 right-1 m-2 px-3 py-2 bg-base-100 rounded-md shadow-sm z-50 text-base-content inline-flex items-center th-topbar ${
            effectiveTopbarVertical ? 'flex-col items-stretch gap-2' : 'flex-row gap-3'
          }`}
        >
          {effectiveTopbarVertical && (
            <button
              type="button"
              onClick={() => void updateSettings({ texthooker_topbar_hidden: true })}
              className="absolute left-0 top-1/2 -translate-x-full -translate-y-1/2 w-4 h-10 flex items-center justify-center bg-base-100 rounded-l-md shadow-sm text-base-content"
              title={t('topbar.hide')}
            >
              <ChevronRight className="w-3.5 h-3.5" />
            </button>
          )}

          {topbarMetrics.map((metric, index) => (
            <Fragment key={metric.key}>
              <div className="font-mono text-base flex items-center gap-2" title={metric.title}>
                {metric.content}
              </div>
              {index < topbarMetrics.length - 1 && (
                <div
                  className={`bg-base-content/20 ${
                    effectiveTopbarVertical ? 'w-full h-px' : 'w-px h-4'
                  }`}
                ></div>
              )}
            </Fragment>
          ))}

          {topbarMetrics.length > 0 && (
            <div
              className={`bg-base-content/20 ${
                effectiveTopbarVertical ? 'w-full h-px my-1' : 'w-px h-4 mx-1'
              }`}
            ></div>
          )}

          {nativeSession || autoSession ? (
            <>
              <div className="join" role="group" aria-label={t('topbar.inputSource')}>
                <button
                  type="button"
                  className={nativeSelected ? 'join-item btn btn-outline btn-sm btn-square relative' : 'join-item btn btn-primary btn-sm btn-square relative'}
                  aria-pressed={!nativeSelected}
                  disabled={changingInput}
                  title={t('topbar.websocket', { status: connectionStatus })}
                  aria-label={t('topbar.websocket', { status: connectionStatus })}
                  onClick={() => nativeSelected
                    ? void changeInputSource('websocket')
                    : void (['connected', 'connecting'].includes(connectionStatus) ? ipc.hookerDisconnect() : ipc.hookerConnect())}
                >
                  {connectionStatus === 'connected' && !nativeSelected ? <LinkIcon className="h-4 w-4" /> : <Unlink className="h-4 w-4" />}
                  {!nativeSelected && <span aria-hidden="true" className={`absolute right-1 top-1 h-1.5 w-1.5 rounded-full ${connectionStatus === 'connected' ? 'bg-success' : connectionStatus === 'connecting' ? 'bg-warning' : connectionStatus === 'error' ? 'bg-error' : 'bg-base-content/40'}`} />}
                </button>
                <button
                  type="button"
                  className={nativeSelected ? 'join-item btn btn-primary btn-sm btn-square relative' : 'join-item btn btn-outline btn-sm btn-square relative'}
                  aria-pressed={nativeSelected}
                  disabled={changingInput}
                  title={t(`hookSelection.status.${nativeSession ? nativeHook?.phase : 'inactive'}`)}
                  aria-label={t('hookSelection.button')}
                  onClick={() => !nativeSession ? openHookPicker() : !nativeSelected && void changeInputSource('lunahook')}
                >
                  <Target className="h-4 w-4" />
                  {nativeSelected && <span aria-hidden="true" className={`absolute right-1 top-1 h-1.5 w-1.5 rounded-full ${nativeHook?.phase === 'error' || nativeHook?.phase === 'unavailable' ? 'bg-error' : nativeHook?.selected_hook_code && ['attached', 'receiving'].includes(nativeHook.phase) ? 'bg-success' : 'bg-warning'}`} />}
                </button>
              </div>
              {nativeSelected && <button type="button" className={topbarIconBtnClass} title={t('hookSelection.choose')} aria-label={t('hookSelection.choose')} onClick={openHookPicker}><SlidersHorizontal className="h-4 w-4" /></button>}
              {inputSourceError && <span className="text-error text-xs" role="alert">{t('topbar.sourceFailed')}</span>}
            </>
          ) : <button
            type="button"
            onClick={() =>
              void (['connected', 'connecting'].includes(connectionStatus) ? ipc.hookerDisconnect() : ipc.hookerConnect())
            }
            className={`${topbarIconBtnClass} transition-colors duration-300 ${
              nativeInput ? 'opacity-40' : connectionStatus === 'connected'
                ? 'text-success'
                : connectionStatus === 'error'
                  ? 'text-error'
                  : connectionStatus === 'connecting'
                    ? 'text-warning'
                    : 'opacity-40'
            }`}
            title={nativeInput ? t('topbar.websocketDisabled') : t('topbar.websocket', { status: connectionStatus })}
          >
            {!nativeInput && connectionStatus === 'connected' ? (
              <LinkIcon className="w-4 h-4" />
            ) : (
              <Unlink className="w-4 h-4" />
            )}
          </button>}

          <button
            type="button"
            onClick={toggleTimer}
            className={topbarIconBtnClass}
            title={
              autoSession
                ? t(autoSession.paused_by_user ? 'topbar.resume' : 'topbar.pause')
                : isTimerActive
                  ? t('topbar.pause')
                  : t('topbar.resume')
            }
          >
            {(autoSession ? !autoSession.paused_by_user : isTimerActive) ? (
              <Pause className="w-4 h-4" />
            ) : (
              <Play className="w-4 h-4" />
            )}
          </button>

          <button
            type="button"
            onClick={() => { setTimerError(false); setIsResetTimerConfirmOpen(true); }}
            className={topbarIconBtnClass}
            title={t('topbar.reset')}
          >
            <RotateCcw className="w-4 h-4" />
          </button>

          <button
            type="button"
            onClick={handleOpenTimerEdit}
            className={topbarIconBtnClass}
            title={t('topbar.setTimer')}
          >
            <Edit3 className="w-4 h-4" />
          </button>

          <button
            type="button"
            onClick={handleDeleteLast}
            className={`${topbarIconBtnClass} text-error`}
            title={t('topbar.deleteLast')}
          >
            <Trash2 className="w-4 h-4" />
          </button>

          <button
            type="button"
            onClick={() => void handleUndo()}
            disabled={undoStack.length === 0}
            className={topbarIconBtnClass}
            title={t('topbar.undo')}
          >
            <Undo2 className="w-4 h-4" />
          </button>

          <button
            type="button"
            onClick={() => setIsStatsOpen(true)}
            className={topbarIconBtnClass}
            title={t('topbar.stats')}
          >
            <BarChart2 className="w-4 h-4" />
          </button>

          <button
            ref={settingsButtonRef}
            type="button"
            onClick={() => setIsSettingsOpen((prev) => !prev)}
            className={topbarIconBtnClass}
            title={t('topbar.settings')}
          >
            <Settings className="w-4 h-4" />
          </button>

          {readingOverlay?.target?.content_id === contentId && <button
            type="button" className={topbarIconBtnClass} aria-pressed={readingOverlay.visible}
            title={t(readingOverlay.visible ? 'readingOverlay:hide' : 'readingOverlay:open')}
            aria-label={t(readingOverlay.visible ? 'readingOverlay:hide' : 'readingOverlay:open')}
            onClick={() => void ipc.readingOverlayAction(readingOverlay.visible ? 'hide' : 'show')}>
            <PanelTop className="w-4 h-4" />
          </button>}

          <button
            type="button"
            onClick={() => setIsSessionPickerOpen(true)}
            className={topbarIconBtnClass}
            title={hooker?.content_title ?? totals?.mediaTitle ?? t('sessions.free')}
          >
            <BookOpen className="w-4 h-4" />
          </button>

          <div
            className={`bg-base-content/20 ${
              effectiveTopbarVertical ? 'w-full h-px my-1' : 'w-px h-4 mx-1'
            }`}
          ></div>

          <button
            type="button"
            onClick={() => getCurrentWindow().label === 'texthooker' ? void getCurrentWindow().hide() : navigate('/')}
            className={topbarIconBtnClass}
            title={getCurrentWindow().label === 'texthooker' ? t('topbar.hideWindow') : t('topbar.back')}
          >
            <Home className="w-4 h-4" />
          </button>

          {roomConnected && (
            <div className="dropdown dropdown-end">
              <button
                type="button"
                tabIndex={0}
                className={`${topbarIconBtnClass} relative`}
                title={t('room.members')}
              >
                <Users className="w-4 h-4" />
                <span className="absolute -top-1 -right-1 badge badge-xs badge-primary scale-75">
                  {roomMembers.length}
                </span>
              </button>
              <ul
                tabIndex={0}
                className="dropdown-content menu menu-sm surface-raised mt-2 p-2 min-w-52 z-50"
              >
                <li className="menu-title px-2 py-1 text-xs opacity-70">
                  {t('room.membersCount', { count: roomMembers.length })}
                </li>
                {roomMembers.map((member) => (
                  <li key={member.id}>
                    <span className="flex justify-between items-center text-xs cursor-default">
                      <span className="flex items-center gap-2">
                        {member.role === 'host' && <Crown className="w-3 h-3 text-warning" />}
                        <span className="font-medium">
                          {member.username ?? t('room.anonymous')}
                        </span>
                      </span>
                      <span className="badge badge-xs capitalize opacity-70">
                        {member.role === 'host' ? t('room.host') : t('room.guest')}
                      </span>
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}

      {/* Timer Edit Modal */}
      <dialog
        className={`modal modal-bottom sm:modal-middle ${isTimerEditOpen ? 'modal-open' : ''}`}
      >
        <div className="modal-box max-w-sm">
          <div className="flex justify-between items-center mb-4">
            <h3 className="font-bold text-lg flex items-center gap-2">
              <Clock className="w-5 h-5 text-primary" />
              {t('timer.title')}
            </h3>
            <button
              onClick={() => setIsTimerEditOpen(false)}
              className="btn btn-ghost btn-sm btn-circle"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          <div className="flex items-center justify-center gap-2 mb-6">
            <Field label={t('timer.hours')}>
              <input
                type="number"
                min="0"
                max="99"
                value={timerEditHours}
                onChange={(e) => setTimerEditHours(Math.max(0, parseInt(e.target.value) || 0))}
                className="input w-20 text-center font-mono text-lg"
              />
            </Field>
            <span className="text-2xl font-bold mt-6">:</span>
            <Field label={t('timer.minutes')}>
              <input
                type="number"
                min="0"
                max="59"
                value={timerEditMinutes}
                onChange={(e) =>
                  setTimerEditMinutes(Math.min(59, Math.max(0, parseInt(e.target.value) || 0)))
                }
                className="input w-20 text-center font-mono text-lg"
              />
            </Field>
            <span className="text-2xl font-bold mt-6">:</span>
            <Field label={t('timer.seconds')}>
              <input
                type="number"
                min="0"
                max="59"
                value={timerEditSeconds}
                onChange={(e) =>
                  setTimerEditSeconds(Math.min(59, Math.max(0, parseInt(e.target.value) || 0)))
                }
                className="input w-20 text-center font-mono text-lg"
              />
            </Field>
          </div>

          <div className="modal-action">
            <button onClick={() => setIsTimerEditOpen(false)} className="btn btn-ghost">
              {t('common:cancel')}
            </button>
            <button disabled={savingTimer} onClick={() => void handleSaveTimerEdit()} className="btn btn-primary">
              {t('common:save')}
            </button>
          </div>
          {timerError && <p role="alert" className="mt-3 text-sm text-error">{t('timer.saveFailed')}</p>}
        </div>
        <form method="dialog" className="modal-backdrop">
          <button onClick={() => setIsTimerEditOpen(false)}>close</button>
        </form>
      </dialog>

      {/* Reset Timer Confirmation Modal */}
      <dialog
        className={`modal modal-bottom sm:modal-middle ${
          isResetTimerConfirmOpen ? 'modal-open' : ''
        }`}
      >
        <div className="modal-box max-w-sm">
          <h3 className="font-bold text-lg mb-2">{t('timer.resetConfirm')}</h3>
          <p className="text-base-content/70 mb-5">
            {t('timer.resetBody')}
          </p>
          <div className="modal-action">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setIsResetTimerConfirmOpen(false)}
            >
              {t('common:cancel')}
            </button>
            <button type="button" disabled={savingTimer} className="btn btn-error" onClick={() => void handleResetTimer()}>
              {t('timer.resetAction')}
            </button>
          </div>
          {timerError && <p role="alert" className="mt-3 text-sm text-error">{t('timer.saveFailed')}</p>}
        </div>
        <form method="dialog" className="modal-backdrop">
          <button onClick={() => setIsResetTimerConfirmOpen(false)}>close</button>
        </form>
      </dialog>

      {/* Stats Modal */}
      <dialog className={`modal modal-bottom sm:modal-middle ${isStatsOpen ? 'modal-open' : ''}`}>
        <div className="modal-box max-w-4xl">
          <div className="flex justify-between items-center mb-4">
            <h3 className="font-bold text-lg flex items-center gap-2">
              <BarChart2 className="w-5 h-5 text-primary" />
              {t('stats.title')}
            </h3>
            <button
              onClick={() => setIsStatsOpen(false)}
              className="btn btn-ghost btn-sm btn-circle"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          <div className="flex flex-col md:flex-row gap-6">
            {totals?.mediaTitle && (
              <div className="md:w-1/3 flex-shrink-0">
                <div className="surface-muted p-4 space-y-4">
                  {cover && <img src={cover} alt={totals.mediaTitle} className="w-full max-h-80 object-contain rounded-box" />}
                  <div>
                    <h4 className="font-bold text-lg line-clamp-2">{totals.mediaTitle}</h4>
                    <p className="text-sm opacity-70 font-mono">{contentId}</p>
                    <div className="flex gap-2 mt-2">
                      <span className="badge badge-primary badge-sm capitalize">
                        {totals.mediaType === 'vn'
                          ? 'visual novel'
                          : totals.mediaType === 'reading'
                            ? 'light novel'
                            : (totals.mediaType ?? t('stats.session'))}
                      </span>
                    </div>
                  </div>
                  <div className="space-y-2 text-sm">
                    <div className="flex items-center justify-between gap-2">
                      <span>{t('stats.progress')}</span>
                      {progressPercent !== null && <strong>{progressPercent.toFixed(1)}%</strong>}
                    </div>
                    {progressPercent !== null ? <>
                      <progress className="progress progress-primary w-full" value={progressPercent} max={100} />
                      <p className="text-xs opacity-70">{t('stats.progressCharacters', { current: numberWithCommas(progressCharacters), total: numberWithCommas(context?.totalCharacters ?? 0) })}</p>
                    </> : <p className="text-xs opacity-70">{t('stats.progressUnavailable')}</p>}
                    {!online && context?.updatedAt && <p className="text-xs opacity-70">{t('stats.offlineCache')}</p>}
                  </div>
                </div>
              </div>
            )}

            <div className="flex-1 space-y-6">
              <div>
                <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
                  {t('stats.current')}
                </h4>
                <div className="grid grid-cols-2 gap-3">
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.time')}</div>
                    <div className="text-xl font-mono font-bold">{formatClock(seconds)}</div>
                  </div>
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.speed')}</div>
                    <div className="text-xl font-bold">
                      {numberWithCommas(readingSpeed(currentSessionChars, seconds))}
                      <span className="text-xs font-normal opacity-60 ml-1">{t('stats.speedUnit')}</span>
                    </div>
                  </div>
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.linesRead')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(currentSessionLines)}</div>
                  </div>
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.characters')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(currentSessionChars)}</div>
                  </div>
                </div>
                <div className="surface-muted p-3 mt-3 space-y-3">
                  <div className="flex items-center gap-2 font-semibold">
                    <Brain className="w-4 h-4 text-primary" /> {t('stats.intelligence.title')}
                  </div>
                  {sessionIntelligence.detectionMode === 'off' ? (
                    <p className="text-sm opacity-70">{t('stats.intelligence.disabled')}</p>
                  ) : (
                    <>
                      <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-sm">
                        <div><span className="opacity-70">{t('stats.intelligence.focus')}</span><div className="font-bold">{currentSessionLines > 1 ? `${sessionIntelligence.focusPercentage}%` : '…'}</div></div>
                        <div><span className="opacity-70">{t('stats.intelligence.focused')}</span><div className="font-bold">{durationLabel(sessionIntelligence.focusedSeconds)}</div></div>
                        <div><span className="opacity-70">{t('stats.intelligence.distracted')}</span><div className="font-bold">{durationLabel(sessionIntelligence.distractedSeconds)}</div></div>
                        <div><span className="opacity-70">{t('stats.intelligence.afk')}</span><div className="font-bold">{durationLabel(sessionIntelligence.afkSeconds)}</div></div>
                      </div>
                      {currentSessionLines < 2 && <p className="text-xs opacity-70">{t('stats.intelligence.waiting')}</p>}
                      {sessionIntelligence.charactersPerMinute.length > 0 && <div className="space-y-1">
                        <div className="flex justify-between gap-2 text-xs opacity-70"><span>{t('stats.intelligence.rate')}</span><span>{Math.max(...sessionIntelligence.charactersPerMinute.slice(-30))}</span></div>
                        <div className="flex items-end gap-1 h-20 border-b border-base-content/20" aria-label={t('stats.intelligence.rate')}>
                          {sessionIntelligence.charactersPerMinute.slice(-30).map((count, index, values) => (
                            <div key={index} className="flex h-full flex-1 items-end justify-center">
                              <div className="w-3 max-w-full bg-primary/70 rounded-t-sm" title={t('stats.intelligence.minute', { minute: Math.max(0, sessionIntelligence.charactersPerMinute.length - 30) + index + 1, count })} style={{ height: `${count / Math.max(1, ...values) * 100}%` }} />
                            </div>
                          ))}
                        </div>
                        <div className="flex justify-between text-xs opacity-60"><span>{t('stats.intelligence.minuteLabel', { minute: Math.max(1, sessionIntelligence.charactersPerMinute.length - 29) })}</span><span>{t('stats.intelligence.minuteLabel', { minute: sessionIntelligence.charactersPerMinute.length })}</span></div>
                      </div>}
                      <div className="text-xs opacity-70">{t('stats.intelligence.distractionCount', { count: sessionIntelligence.distractionCount })}</div>
                    </>
                  )}
                </div>
              </div>

              <div className="divider"></div>

              <div>
                <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
                  {t('stats.total')}
                </h4>
                <div className="grid grid-cols-2 gap-3">
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.totalLines')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(allTimeLines)}</div>
                  </div>
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.totalCharacters')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(allTimeChars)}</div>
                  </div>
                </div>
              </div>

              <div className="divider"></div>

              <div>
                <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
                  {t('stats.history')}
                </h4>
                {(history ?? []).length === 0 ? (
                  <div className="surface-muted p-3 text-sm opacity-70">
                    {t('stats.noHistory')}
                  </div>
                ) : (
                  <div className="space-y-2 max-h-72 overflow-y-auto pr-1">
                    {(history ?? []).map((entry, index) => (
                      <div key={`${entry.loggedAt}-${index}`} className="surface-muted p-3">
                        <button type="button" className="text-xs opacity-70 mb-2 hover:text-primary" onClick={() => setExpandedIntelligenceId(expandedIntelligenceId === entry.id ? null : entry.id)}>
                          {new Date(entry.loggedAt).toLocaleString()}
                          {entry.intelligence && <span className="ml-2 text-primary">{t('stats.intelligence.title')}</span>}
                        </button>
                        <div className="grid grid-cols-2 gap-2 text-sm">
                          <div className="flex justify-between">
                            <span className="opacity-70">{t('stats.characters')}</span>
                            <span className="font-semibold">
                              {numberWithCommas(entry.charactersLogged)}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="opacity-70">{t('stats.speed')}</span>
                            <span className="font-semibold">
                              {numberWithCommas(entry.readingSpeed)} {t('stats.speedUnit')}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="opacity-70">{t('stats.time')}</span>
                            <span className="font-semibold">
                              {formatClock(entry.sessionSeconds)}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="opacity-70">{t('stats.lines')}</span>
                            <span className="font-semibold">
                              {numberWithCommas(entry.linesLogged)}
                            </span>
                          </div>
                        </div>
                        {entry.intelligence && expandedIntelligenceId === entry.id && (
                          <div className="mt-3 pt-3 border-t border-base-content/10 space-y-2">
                            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-sm">
                              <div>{t('stats.intelligence.focus')} <strong>{entry.intelligence.focusPercentage}%</strong></div>
                              <div>{t('stats.intelligence.focused')} <strong>{durationLabel(entry.intelligence.focusedSeconds)}</strong></div>
                              <div>{t('stats.intelligence.distracted')} <strong>{durationLabel(entry.intelligence.distractedSeconds)}</strong></div>
                              <div>{t('stats.intelligence.afk')} <strong>{durationLabel(entry.intelligence.afkSeconds)}</strong></div>
                            </div>
                            <div className="flex items-end gap-1 h-12">
                              {(entry.intelligence.charactersPerMinute.length ? entry.intelligence.charactersPerMinute : [0]).map((count, barIndex, values) => (
                                <div key={barIndex} className="flex-1 bg-primary/70 rounded-t-sm min-w-1" title={`${count} ${t('stats.intelligence.charsPerMinute')}`} style={{ height: `${Math.max(3, (count / Math.max(1, ...values)) * 100)}%` }} />
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <Field label={t('stats.comment')}>
                {(id) => (
                  <textarea
                    id={id}
                    className="textarea w-full"
                    rows={3}
                    value={logComment}
                    onChange={(event) => setLogComment(event.target.value)}
                    placeholder={t('stats.commentPlaceholder')}
                  />
                )}
              </Field>

              <div className="flex justify-end pt-4 border-t border-base-content/10">
                <button
                  onClick={() => void handleLogSession()}
                  disabled={isLogging || currentSessionChars === 0}
                  className={`btn btn-primary w-full sm:w-auto transition-all ${
                    isLogAnimating ? 'btn-success scale-105' : ''
                  }`}
                >
                  {isLogging ? (
                    <span className="loading loading-spinner loading-md"></span>
                  ) : isLogAnimating ? (
                    <>
                      <span className="text-xl">✓</span>
                      {t('stats.logged')}
                    </>
                  ) : (
                    <>
                      <Save className="w-5 h-5" />
                      {t('stats.logSession')}
                    </>
                  )}
                </button>
              </div>
            </div>
          </div>
        </div>
        <form method="dialog" className="modal-backdrop">
          <button onClick={() => setIsStatsOpen(false)}>close</button>
        </form>
      </dialog>

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
                label="Pausa automática (segundos sin líneas)"
                hint="0 la desactiva. No afecta a las sesiones que lleva la detección de novelas."
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
                  En ejecución
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
                        onClick={() => {
                          void navigator.clipboard.writeText(inviteLink).then(() => {
                            setInviteCopied(true);
                            setTimeout(() => setInviteCopied(false), 2000);
                          });
                        }}
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
        {lines.map((line) => (
          <LineItem
            key={line.id}
            line={line}
            vertical={vertical}
            textStyle={lineTextStyle}
            saveLabel={t('mining.saveLine')}
            deleteLabel={t('deleteLine')}
            onPointer={openDictionaryAtPointer}
            onLeave={cancelDictionaryHover}
            onSave={saveLine}
            onDelete={deleteLine}
          />
        ))}
      </div>

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
          onClose={() => {
            setDictionaryRequest(null);
            setDictionaryAnchor(null);
          }}
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
          <NativeHookChoices status={nativeHook} title={tracker?.vn?.media_title} selected={selectedHookCode} onSelect={setSelectedHookCode}
            enable={!nativeHook && tracker?.vn && !hookApp?.native_hook_enabled ? { busy: enablingHook, disabled: !hookApp || hookApps.isPending, onClick: () => void enableNativeHook() } : undefined}
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
