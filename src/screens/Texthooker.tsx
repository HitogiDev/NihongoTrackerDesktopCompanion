import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import {
  Activity,
  BarChart2,
  BookOpen,
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
  Play,
  RotateCcw,
  Save,
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
import { MediaSearchModal } from '../components/MediaSearchModal';
import { formatClock, numberWithCommas, readingSpeed } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import {
  RoomConnection,
  randomRoomId,
  type RoomLine,
  type RoomMember,
} from '../lib/room';
import type { HookerTotals, TextLine, TextSessionSummary } from '../lib/types';

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
  { value: 'retro', label: 'Retro' },
  { value: 'garden', label: 'Garden' },
  { value: 'forest', label: 'Forest' },
  { value: 'winter', label: 'Winter' },
  { value: 'sunset', label: 'Sunset' },
];

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

  const contentId = hooker?.content_id ?? LOCAL_SESSION;

  const [lines, setLines] = useState<TextLine[]>([]);
  const [totals, setTotals] = useState<HookerTotals | null>(null);
  const [undoStack, setUndoStack] = useState<TextLine[][]>([]);

  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<'display' | 'behavior'>('display');
  const [isStatsOpen, setIsStatsOpen] = useState(false);
  const [isTimerEditOpen, setIsTimerEditOpen] = useState(false);
  const [isResetTimerConfirmOpen, setIsResetTimerConfirmOpen] = useState(false);
  const [isClassHelpOpen, setIsClassHelpOpen] = useState(false);
  const [isSessionPickerOpen, setIsSessionPickerOpen] = useState(false);
  const [attaching, setAttaching] = useState(false);
  const [isLogging, setIsLogging] = useState(false);
  const [isLogAnimating, setIsLogAnimating] = useState(false);

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
  const topbarRef = useRef<HTMLDivElement>(null);
  const settingsPanelRef = useRef<HTMLDivElement>(null);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const topbarNaturalWidthRef = useRef(0);
  const isTopbarVerticalRef = useRef(false);
  const [isTopbarVertical, setIsTopbarVertical] = useState(false);

  // A detected visual novel drives the clock from Rust; the manual timer is for
  // everything else.
  const autoSession = tracker?.vn && tracker.vn.content_id === contentId ? tracker.vn : null;
  const seconds = autoSession ? autoSession.elapsed_secs : manualSeconds;

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
  const effectiveTopbarVertical = forceTopbarVertical || isTopbarVertical;
  const connectionStatus = hooker?.status ?? 'disconnected';
  const roomMode = (settings?.texthooker_room_mode ?? 'local') as 'local' | 'host' | 'guest';
  const roomId = settings?.texthooker_room_id ?? '';
  const inviteLink =
    roomId && settings?.api_base_url
      ? `${settings.api_base_url.replace(/\/+$/, '')}/texthooker/session?mode=guest&roomId=${roomId}`
      : '';

  // ── data ────────────────────────────────────────────────────────────

  const reload = useCallback(async () => {
    const [nextLines, nextTotals] = await Promise.all([
      ipc.hookerLines(contentId, 5000),
      ipc.hookerTotals(contentId),
    ]);
    setLines(nextLines);
    setTotals(nextTotals);
    if (!autoSession) {
      setManualSeconds(nextTotals.timerSeconds);
      secondsRef.current = nextTotals.timerSeconds;
      anchorRef.current = { at: Date.now(), seconds: nextTotals.timerSeconds };
    }
  }, [contentId, autoSession]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const unlisten = listen<{ line: TextLine; totals: { lines: number; chars: number } }>(
      events.hookerLine,
      (event) => {
        if (event.payload.line.content_id !== contentId) return;
        lastActivityRef.current = Date.now();
        setLines((prev) => [...prev, event.payload.line]);
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
      void unlisten.then((fn) => fn());
    };
  }, [contentId]);

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
    if (autoSession) return;
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
    setIsTimerEditOpen(true);
  }, [seconds]);

  const handleSaveTimerEdit = useCallback(() => {
    const total = timerEditHours * 3600 + timerEditMinutes * 60 + timerEditSeconds;
    secondsRef.current = total;
    setManualSeconds(total);
    anchorTimer(total);
    lastActivityRef.current = Date.now();
    void ipc.hookerSetTimer(contentId, total);
    setIsTimerEditOpen(false);
  }, [timerEditHours, timerEditMinutes, timerEditSeconds, contentId, anchorTimer]);

  const handleResetTimer = useCallback(() => {
    secondsRef.current = 0;
    setManualSeconds(0);
    anchorTimer(0);
    setIsTimerActive(false);
    void ipc.hookerSetTimer(contentId, 0);
    setIsResetTimerConfirmOpen(false);
  }, [contentId, anchorTimer]);

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
    if (lines.length === 0) return;
    setUndoStack((prev) => [...prev, lines]);
    await ipc.hookerClear(contentId);
    await reload();
  }, [lines, contentId, reload]);

  const handleCopyAll = useCallback(async () => {
    const combined = lines.map((line) => line.text).join('\n');
    if (!combined) return;
    await navigator.clipboard.writeText(combined).catch(() => undefined);
  }, [lines]);

  // ── derived ─────────────────────────────────────────────────────────

  const linesNumber = totals?.lines ?? lines.length;
  const charsNumber = totals?.chars ?? 0;
  const currentSessionChars = autoSession?.session_chars ?? totals?.sessionChars ?? 0;
  const currentSessionLines = autoSession?.session_lines ?? totals?.sessionLines ?? 0;

  useEffect(() => {
    const el = listContainerRef.current;
    if (!el) return;
    if (vertical) el.scrollLeft = 0;
    else el.scrollTop = el.scrollHeight;
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

  const textBaseStyle: CSSProperties = {
    lineHeight: lineSpacing,
    fontSize,
    fontFamily:
      FONT_FAMILY_STACKS[fontFamily] ||
      `"${fontFamily}", "Noto Sans JP Local", "Noto Sans JP", sans-serif`,
  };

  const verticalTextStyle: CSSProperties = {
    ...textBaseStyle,
    writingMode: 'vertical-rl',
    textOrientation: 'mixed',
    flexShrink: 0,
    height: '100%',
    direction: 'ltr',
  };

  const horizontalTextStyle: CSSProperties = { ...textBaseStyle, display: 'block' };

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
        chars: currentSessionChars,
        seconds,
      });
      setIsLogAnimating(true);
      setTimeout(() => setIsLogAnimating(false), 2000);
      secondsRef.current = 0;
      setManualSeconds(0);
      anchorTimer(0);
      setIsTimerActive(false);
      await reload();
      void queryClient.invalidateQueries({ queryKey: ['outbox'] });
      void queryClient.invalidateQueries({ queryKey: ['text-history', contentId] });
    } finally {
      setIsLogging(false);
    }
  }

  const { data: history } = useQuery({
    queryKey: ['text-history', contentId],
    queryFn: () => ipc.hookerHistory(contentId),
  });

  return (
    <div data-theme={effectiveHookerTheme} className="bg-base-300 min-h-screen th-root">
      {customCss.trim() && <style>{customCss}</style>}

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

          <button
            type="button"
            onClick={() =>
              void (connectionStatus === 'connected' ? ipc.hookerDisconnect() : ipc.hookerConnect())
            }
            className={`${topbarIconBtnClass} transition-colors duration-300 ${
              connectionStatus === 'connected'
                ? 'text-success'
                : connectionStatus === 'error'
                  ? 'text-error'
                  : connectionStatus === 'connecting'
                    ? 'text-warning'
                    : 'opacity-40'
            }`}
            title={t('topbar.websocket', { status: connectionStatus })}
          >
            {connectionStatus === 'connected' ? (
              <LinkIcon className="w-4 h-4" />
            ) : (
              <Unlink className="w-4 h-4" />
            )}
          </button>

          <button
            type="button"
            onClick={toggleTimer}
            disabled={Boolean(autoSession)}
            className={topbarIconBtnClass}
            title={
              autoSession
                ? t('topbar.autoTimer')
                : isTimerActive
                  ? t('topbar.pause')
                  : t('topbar.resume')
            }
          >
            {(autoSession ? autoSession.counting : isTimerActive) ? (
              <Pause className="w-4 h-4" />
            ) : (
              <Play className="w-4 h-4" />
            )}
          </button>

          <button
            type="button"
            onClick={() => setIsResetTimerConfirmOpen(true)}
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
            onClick={() => navigate('/')}
            className={topbarIconBtnClass}
            title={t('topbar.back')}
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
            <button onClick={handleSaveTimerEdit} className="btn btn-primary">
              {t('common:save')}
            </button>
          </div>
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
            <button type="button" className="btn btn-error" onClick={handleResetTimer}>
              {t('timer.resetAction')}
            </button>
          </div>
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
              </div>

              <div className="divider"></div>

              <div>
                <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
                  {t('stats.total')}
                </h4>
                <div className="grid grid-cols-2 gap-3">
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.totalLines')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(linesNumber)}</div>
                  </div>
                  <div className="surface-muted p-3">
                    <div className="text-xs opacity-70 mb-1">{t('stats.totalCharacters')}</div>
                    <div className="text-xl font-bold">{numberWithCommas(charsNumber)}</div>
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
                        <div className="flex items-center justify-between gap-2 mb-2">
                          <span className="text-xs opacity-70">
                            {new Date(entry.loggedAt).toLocaleString()}
                          </span>
                        </div>
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
                      </div>
                    ))}
                  </div>
                )}
              </div>

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
                    <span className="text-sm">{t('settings.preventDuplicates')}</span>
                    <input
                      type="checkbox"
                      className="toggle toggle-primary toggle-sm"
                      checked={settings?.texthooker_prevent_duplicates ?? false}
                      onChange={(e) =>
                        void updateSettings({ texthooker_prevent_duplicates: e.target.checked })
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
                  className="btn btn-outline btn-sm"
                >
                  <Copy className="w-4 h-4" /> {t('settings.copyAllLines')}
                </button>
                <button
                  type="button"
                  onClick={() => void handleClearAll()}
                  className="btn btn-error btn-outline btn-sm"
                >
                  <Trash2 className="w-4 h-4" /> {t('settings.clearAllLines')}
                </button>
              </div>
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
      >
        {lines.map((line) => {
          if (vertical) {
            return (
              <div
                key={line.id}
                className="group relative flex flex-col items-end justify-start px-2 pt-6 text-base-content shrink-0 th-line-item"
                style={{ minHeight: COLUMN_HEIGHT, height: COLUMN_HEIGHT }}
              >
                <span
                  lang="ja"
                  className="block text-lg whitespace-pre-wrap wrap-break-word th-line-text"
                  style={verticalTextStyle}
                >
                  {line.text}
                  <button
                    type="button"
                    className="inline-flex justify-center items-center text-error text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
                    aria-label={t('deleteLine')}
                    onClick={() => void handleDelete(line.id)}
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
            <div
              key={line.id}
              className="group relative px-2 py-2 text-base-content shrink-0 th-line-item"
            >
              <p
                className="whitespace-pre-wrap wrap-break-word th-line-text"
                style={horizontalTextStyle}
              >
                {line.text}
                <button
                  type="button"
                  className="ml-2 align-baseline text-error text-sm bg-base-100/80 rounded px-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
                  aria-label={t('deleteLine')}
                  onClick={() => void handleDelete(line.id)}
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </p>
            </div>
          );
        })}
      </div>

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
          await reload();
        }}
        onSearch={() => {
          setIsSessionPickerOpen(false);
          setAttaching(true);
        }}
      />

      <MediaSearchModal
        open={attaching}
        onClose={() => setAttaching(false)}
        mediaType={totals?.mediaType ?? 'vn'}
        initialQuery={hooker?.content_title ?? ''}
        onPick={async (hit, type) => {
          await ipc.hookerAttachMedia(
            contentId,
            hit.contentId,
            hit.title.contentTitleRomaji || hit.title.contentTitleNative,
            hit.type ?? type
          );
          setAttaching(false);
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
