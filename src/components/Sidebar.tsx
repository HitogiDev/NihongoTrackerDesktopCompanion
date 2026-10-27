import { useEffect } from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  Activity,
  BookOpenText,
  CloudOff,
  Cloud,
  Inbox,
  Library,
  BookmarkPlus,
  Settings as SettingsIcon,
  Plug,
  Target,
  Trophy,
  UsersRound,
  type LucideIcon,
} from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { ipc } from '../lib/ipc';
import { shortcutLabel } from '../lib/desktop';
import { useQuickLogStore } from '../store/useQuickLogStore';
import { UpdateNotice } from './UpdateSettings';

type NavItem = { to: string; labelKey: string; icon: LucideIcon; shortcut: string; end?: boolean; badge?: 'pending' };

const TRACKER_NAV: NavItem[] = [
  { to: '/', labelKey: 'activity', icon: Activity, shortcut: '1', end: true },
  { to: '/queue', labelKey: 'pending', icon: Inbox, shortcut: '2', badge: 'pending' },
  { to: '/library', labelKey: 'library', icon: Library, shortcut: '3' },
  { to: '/mining', labelKey: 'mining', icon: BookmarkPlus, shortcut: '4' },
];

const COMMUNITY_NAV: NavItem[] = [
  { to: '/social', labelKey: 'social', icon: UsersRound, shortcut: '5' },
  { to: '/ranking', labelKey: 'ranking', icon: Trophy, shortcut: '6' },
];

const NAV_BY_SHORTCUT = new Map([...TRACKER_NAV, ...COMMUNITY_NAV].map((item) => [item.shortcut, item.to]));

export function Sidebar() {
  const { t } = useTranslation('nav');
  // Primitive selectors: the tracker snapshot is replaced every second, but
  // the sidebar only changes when one of these values does.
  const hookerStatus = useAppStore((s) => s.hooker?.status);
  const online = useAppStore((s) => s.online);
  const user = useAppStore((s) => s.user);
  const pending = useAppStore((s) => s.tracker?.pending_count ?? 0);
  const queued = useAppStore((s) => s.tracker?.queued_count ?? 0);
  const hasVn = useAppStore((s) => Boolean(s.tracker?.vn));
  const nativePhase = useAppStore((s) => s.tracker?.game_hook.phase);
  const websocketSelected = useAppStore((s) => s.tracker?.game_hook.websocket_selected);
  const nativeActive = !websocketSelected
    && (nativePhase === 'starting' || nativePhase === 'attached' || nativePhase === 'receiving');
  const navigate = useNavigate();
  const openQuickLog = useQuickLogStore((s) => s.openForm);

  // Window shortcuts, live wherever the sidebar is: Ctrl/⌘+1…6 for the
  // sections, +, for settings, +L for the log form, +T for the texthooker.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      const key = event.key.toLowerCase();
      const route = NAV_BY_SHORTCUT.get(key);
      if (route) navigate(route);
      else if (key === ',') void ipc.openAuxWindow('settings');
      else if (key === 'l') openQuickLog('manual');
      else if (key === 't') void ipc.openWorkTexthooker();
      else return;
      event.preventDefault();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [navigate, openQuickLog]);

  function renderNavItem({ to, labelKey, icon: Icon, shortcut, end, badge }: NavItem) {
    return (
      <li key={to}>
        <NavLink
          to={to}
          end={end}
          title={t('shortcutHint', { label: t(labelKey), keys: shortcutLabel(shortcut) })}
          className={({ isActive }) =>
            `gap-3 rounded-field px-3 py-2 text-sm font-medium transition-colors ${
              isActive ? 'bg-primary/10 text-primary' : 'text-base-content/75 hover:bg-base-200 hover:text-base-content'
            }`
          }
        >
          <Icon className="h-4 w-4" />
          <span className="flex-1">{t(labelKey)}</span>
          {badge === 'pending' && pending > 0 ? (
            <span className="badge badge-sm badge-warning">{pending}</span>
          ) : null}
        </NavLink>
      </li>
    );
  }

  return (
    <aside className="app-sidebar flex w-56 shrink-0 flex-col border-r border-base-300/70 bg-base-100">
      <nav className="flex-1 overflow-y-auto px-3 py-4">
        <p className="mb-2 px-3 text-[10px] font-semibold uppercase tracking-[0.16em] text-base-content/45">{t('trackerGroup')}</p>
        <ul className="menu menu-sm w-full gap-1 p-0">
          {TRACKER_NAV.map(renderNavItem)}
          <li>
            <button type="button" title={t('shortcutHint', { label: t('openTexthooker'), keys: shortcutLabel('T') })} className="gap-3 rounded-field px-3 py-2 text-sm font-medium text-base-content/75 transition-colors hover:bg-base-200 hover:text-base-content" onClick={() => void ipc.openWorkTexthooker()}>
              <BookOpenText className="h-4 w-4" />
              <span>{t('openTexthooker')}</span>
            </button>
          </li>
        </ul>
        <p className="mb-2 mt-7 px-3 text-[10px] font-semibold uppercase tracking-[0.16em] text-base-content/45">{t('communityGroup')}</p>
        <ul className="menu menu-sm w-full gap-1 p-0">{COMMUNITY_NAV.map(renderNavItem)}</ul>
      </nav>

      <div className="space-y-3 border-t border-base-300/70 p-4">
        <UpdateNotice />
        {user?.username ? (
          <div className="flex min-w-0 items-center gap-2.5">
            <div className="grid h-8 w-8 shrink-0 place-items-center overflow-hidden rounded-full bg-base-200 text-xs font-semibold">
              {user.avatar ? <img src={user.avatar} alt="" className="h-full w-full object-cover" /> : user.username.slice(0, 1).toUpperCase()}
            </div>
            <div className="min-w-0 flex-1">
              <span className="block truncate text-sm font-medium">{user.username}</span>
              <span className="text-xs text-base-content/50">{t('account')}</span>
            </div>
            <button type="button" onClick={() => void ipc.openAuxWindow('settings')} aria-label={t('settings')} title={t('shortcutHint', { label: t('settings'), keys: shortcutLabel(',') })} className="btn btn-ghost btn-sm btn-square">
              <SettingsIcon className="h-4 w-4" />
            </button>
          </div>
        ) : (
          <button type="button" onClick={() => void ipc.openAuxWindow('settings')} className="btn btn-ghost btn-sm w-full justify-start gap-3"><SettingsIcon className="h-4 w-4" />{t('settings')}</button>
        )}
        <div className="flex items-center gap-2 text-xs text-base-content/60">
          {online ? <Cloud className="h-3.5 w-3.5 text-success" /> : <CloudOff className="h-3.5 w-3.5 text-warning" />}
          <span>{online === null ? t('status.checking') : online ? t('status.online') : t('status.offline')}</span>
          {queued > 0 && <span className="badge badge-xs badge-warning ml-auto">{queued}</span>}
        </div>
        <div className="flex items-center gap-2 text-xs text-base-content/60">
          {nativeActive ? <>
            <Target className={`h-3.5 w-3.5 ${nativePhase === 'starting' ? 'text-warning' : 'text-success'}`} />
            <span className="truncate">{nativePhase === 'starting' ? t('status.nativeConnecting') : t('status.nativeConnected')}</span>
          </> : <>
            <Plug className={`h-3.5 w-3.5 ${hookerStatus === 'connected' ? 'text-success' : hookerStatus === 'connecting' ? 'text-warning' : 'text-base-content/40'}`} />
            <span className="truncate">{hookerStatus === 'connected' ? t('status.hookerConnected') : hookerStatus === 'connecting' ? t('status.hookerConnecting') : !hasVn && hookerStatus !== 'error' ? t('status.hookerIdle') : t('status.hookerOffline')}</span>
          </>}
        </div>
        {pending > 0 && <span className="sr-only">{t('pendingCount', { count: pending })}</span>}
      </div>
    </aside>
  );
}
