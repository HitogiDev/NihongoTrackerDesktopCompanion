import { lazy, Suspense, useEffect, useState } from 'react';
import { Link, NavLink, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  BookOpenText,
  CloudOff,
  Cloud,
  Search,
  Settings as SettingsIcon,
  Plug,
  Target,
} from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { ipc } from '../lib/ipc';
import { shortcutLabel } from '../lib/desktop';
import { useQuickLogStore } from '../store/useQuickLogStore';
import { UpdateNotice } from './UpdateSettings';
import { COMMUNITY_NAV, TRACKER_NAV, type NavItem } from './navItems';

// Its own chunk: every window loads this bundle, and only `main` has a palette.
const CommandPalette = lazy(() => import('./CommandPalette'));

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
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Mounted on first use and kept, so reopening it is instant.
  const [paletteUsed, setPaletteUsed] = useState(false);
  const openPalette = () => {
    setPaletteUsed(true);
    setPaletteOpen(true);
  };

  // Window shortcuts, live wherever the sidebar is: Ctrl/⌘+1…6 for the
  // sections, +, for settings, +L for the log form, +T for the texthooker,
  // +K for the command palette.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      const key = event.key.toLowerCase();
      const route = NAV_BY_SHORTCUT.get(key);
      if (route) navigate(route);
      else if (key === ',') void ipc.openAuxWindow('settings');
      else if (key === 'l') openQuickLog('manual');
      else if (key === 't') void ipc.openWorkTexthooker();
      else if (key === 'k') {
        setPaletteUsed(true);
        setPaletteOpen((open) => !open);
      } else return;
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
        <button
          type="button"
          title={t('shortcutHint', { label: t('palette.title'), keys: shortcutLabel('K') })}
          className="btn btn-ghost btn-sm mb-4 w-full justify-start gap-3 border border-base-300/70 font-normal text-base-content/60"
          onClick={openPalette}
        >
          <Search className="h-4 w-4" />
          <span className="min-w-0 flex-1 truncate text-left">{t('palette.open')}</span>
          <kbd className="kbd kbd-xs">{shortcutLabel('K')}</kbd>
        </button>
        <p className="mb-2 px-3 text-[10px] font-semibold uppercase tracking-[0.16em] text-base-content/45">{t('trackerGroup')}</p>
        <ul className="menu menu-sm w-full gap-1 p-0">
          {TRACKER_NAV.map(renderNavItem)}
        </ul>
        <p className="mb-2 mt-7 px-3 text-[10px] font-semibold uppercase tracking-[0.16em] text-base-content/45">{t('communityGroup')}</p>
        <ul className="menu menu-sm w-full gap-1 p-0">{COMMUNITY_NAV.map(renderNavItem)}</ul>
        {/* An action, not a place: it opens its own window, so it sits apart from the sections. */}
        <button type="button" title={t('shortcutHint', { label: t('openTexthooker'), keys: shortcutLabel('T') })} className="btn btn-outline btn-sm mt-7 w-full justify-start gap-3" onClick={() => void ipc.openWorkTexthooker()}>
          <BookOpenText className="h-4 w-4" />
          {t('openTexthooker')}
        </button>
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
          {queued > 0 && <Link to="/queue" className="link link-hover ml-auto text-warning">{t('status.queued', { count: queued })}</Link>}
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
      {paletteUsed ? (
        <Suspense fallback={null}>
          <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
        </Suspense>
      ) : null}
    </aside>
  );
}
