import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Activity,
  BarChart2,
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Clock,
  Crown,
  Edit3,
  FileText,
  Home,
  Link as LinkIcon,
  Pause,
  PanelTop,
  Play,
  RotateCcw,
  Settings,
  SlidersHorizontal,
  Target,
  Trash2,
  Type,
  Undo2,
  Unlink,
  Users,
} from 'lucide-react';
import { useReadingOverlay } from '../../hooks/useReadingOverlay';
import { numberWithCommas } from '../../lib/format';
import { ipc } from '../../lib/ipc';
import type { RoomMember } from '../../lib/room';
import type { NativeGameHookStatus } from '../../lib/types';
import { useAppStore } from '../../store/useAppStore';
import { SessionClock, SessionSpeed } from './SessionClock';
import type { SessionClockSource } from './useManualTimer';

interface TopBarProps {
  settingsButtonRef: RefObject<HTMLButtonElement | null>;
  clock: SessionClockSource;
  contentId: string;
  sessionTitle: string;
  currentSessionChars: number;
  currentSessionLines: number;
  charsNumber: number;
  linesNumber: number;
  nativeHook: NativeGameHookStatus | null;
  nativeSession: boolean;
  nativeSelected: boolean;
  nativeInput: boolean;
  changingInput: boolean;
  inputSourceError: boolean;
  /** `null` without a detected session for this work. */
  autoPausedByUser: boolean | null;
  isTimerActive: boolean;
  canUndo: boolean;
  roomConnected: boolean;
  roomMembers: RoomMember[];
  onChangeInputSource: (source: 'lunahook' | 'websocket') => void;
  onOpenHookPicker: () => void;
  onToggleTimer: () => void;
  onResetTimer: () => void;
  onEditTimer: () => void;
  onDeleteLast: () => void;
  onUndo: () => void;
  onOpenStats: () => void;
  onToggleSettings: () => void;
  onOpenSessions: () => void;
}

/** The floating bar: session metrics, input source, timer and screen actions. */
export function TopBar({
  settingsButtonRef, clock, contentId, sessionTitle,
  currentSessionChars, currentSessionLines, charsNumber, linesNumber,
  nativeHook, nativeSession, nativeSelected, nativeInput, changingInput, inputSourceError,
  autoPausedByUser, isTimerActive, canUndo, roomConnected, roomMembers,
  onChangeInputSource, onOpenHookPicker, onToggleTimer, onResetTimer, onEditTimer,
  onDeleteLast, onUndo, onOpenStats, onToggleSettings, onOpenSessions,
}: TopBarProps) {
  const { t } = useTranslation(['texthooker', 'common']);
  const navigate = useNavigate();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const connectionStatus = useAppStore((s) => s.hooker?.status ?? 'disconnected');
  const readingOverlay = useReadingOverlay();

  const topbarRef = useRef<HTMLDivElement>(null);
  const topbarNaturalWidthRef = useRef(0);
  const isTopbarVerticalRef = useRef(false);
  const [isTopbarVertical, setIsTopbarVertical] = useState(false);
  const isTopbarHidden = settings?.texthooker_topbar_hidden ?? false;
  const forceTopbarVertical = settings?.texthooker_force_topbar_vertical ?? false;
  const effectiveTopbarVertical = forceTopbarVertical || isTopbarVertical;

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

  const topbarMetrics: Array<{ key: string; title: string; content: ReactNode }> = [];
  if (settings?.texthooker_show_timer) {
    topbarMetrics.push({
      key: 'timer',
      title: t('topbar.timer'),
      content: (
        <>
          <Clock className="w-4 h-4 opacity-70" />
          <SessionClock clock={clock} />
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
          <SessionSpeed clock={clock} chars={currentSessionChars} suffix="/h" />
        </>
      ),
    });
  }
  if (settings?.texthooker_show_characters) {
    topbarMetrics.push({
      key: 'characters',
      title: t('topbar.charactersDetail', {
        session: numberWithCommas(currentSessionChars),
        total: numberWithCommas(charsNumber),
      }),
      content: (
        <>
          <Type className="w-4 h-4 opacity-70" />
          {numberWithCommas(currentSessionChars)}
        </>
      ),
    });
  }
  if (settings?.texthooker_show_lines) {
    topbarMetrics.push({
      key: 'lines',
      title: t('topbar.linesDetail', {
        session: numberWithCommas(currentSessionLines),
        total: numberWithCommas(linesNumber),
      }),
      content: (
        <>
          <FileText className="w-4 h-4 opacity-70" />
          {numberWithCommas(currentSessionLines)}
        </>
      ),
    });
  }

  const topbarIconBtnClass = effectiveTopbarVertical
    ? 'btn btn-xs btn-ghost w-full justify-center'
    : 'btn btn-xs btn-ghost btn-square';

  return (
    isTopbarHidden && effectiveTopbarVertical ? (
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

        {nativeSession || autoPausedByUser !== null ? (
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
                  ? onChangeInputSource('websocket')
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
                onClick={() => !nativeSession ? onOpenHookPicker() : !nativeSelected && onChangeInputSource('lunahook')}
              >
                <Target className="h-4 w-4" />
                {nativeSelected && <span aria-hidden="true" className={`absolute right-1 top-1 h-1.5 w-1.5 rounded-full ${nativeHook?.phase === 'error' || nativeHook?.phase === 'unavailable' ? 'bg-error' : nativeHook?.selected_hook_code && ['attached', 'receiving'].includes(nativeHook.phase) ? 'bg-success' : 'bg-warning'}`} />}
              </button>
            </div>
            {nativeSelected && <button type="button" className={topbarIconBtnClass} title={t('hookSelection.choose')} aria-label={t('hookSelection.choose')} onClick={onOpenHookPicker}><SlidersHorizontal className="h-4 w-4" /></button>}
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
          onClick={onToggleTimer}
          className={topbarIconBtnClass}
          title={
            autoPausedByUser !== null
              ? t(autoPausedByUser ? 'topbar.resume' : 'topbar.pause')
              : isTimerActive
                ? t('topbar.pause')
                : t('topbar.resume')
          }
        >
          {(autoPausedByUser !== null ? !autoPausedByUser : isTimerActive) ? (
            <Pause className="w-4 h-4" />
          ) : (
            <Play className="w-4 h-4" />
          )}
        </button>

        <button
          type="button"
          onClick={onResetTimer}
          className={topbarIconBtnClass}
          title={t('topbar.reset')}
        >
          <RotateCcw className="w-4 h-4" />
        </button>

        <button
          type="button"
          onClick={onEditTimer}
          className={topbarIconBtnClass}
          title={t('topbar.setTimer')}
        >
          <Edit3 className="w-4 h-4" />
        </button>

        <button
          type="button"
          onClick={onDeleteLast}
          className={`${topbarIconBtnClass} text-error`}
          title={t('topbar.deleteLast')}
        >
          <Trash2 className="w-4 h-4" />
        </button>

        <button
          type="button"
          onClick={onUndo}
          disabled={!canUndo}
          className={topbarIconBtnClass}
          title={t('topbar.undo')}
        >
          <Undo2 className="w-4 h-4" />
        </button>

        <button
          type="button"
          onClick={onOpenStats}
          className={topbarIconBtnClass}
          title={t('topbar.stats')}
        >
          <BarChart2 className="w-4 h-4" />
        </button>

        <button
          ref={settingsButtonRef}
          type="button"
          onClick={onToggleSettings}
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
          onClick={onOpenSessions}
          className={topbarIconBtnClass}
          title={sessionTitle}
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
    )
  );
}
