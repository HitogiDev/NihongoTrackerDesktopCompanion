import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  Activity,
  BookOpenText,
  CloudOff,
  Cloud,
  Inbox,
  Library,
  Settings as SettingsIcon,
  Plug,
  Trophy,
} from 'lucide-react';
import { useAppStore } from '../store/useAppStore';

const NAV = [
  { to: '/', labelKey: 'activity', icon: Activity, end: true },
  { to: '/queue', labelKey: 'pending', icon: Inbox, badge: 'pending' as const },
  { to: '/library', labelKey: 'library', icon: Library },
  { to: '/ranking', labelKey: 'ranking', icon: Trophy },
  { to: '/texthooker', labelKey: 'texthooker', icon: BookOpenText },
  { to: '/settings', labelKey: 'settings', icon: SettingsIcon },
];

export function Sidebar() {
  const { t } = useTranslation('nav');
  const tracker = useAppStore((s) => s.tracker);
  const hooker = useAppStore((s) => s.hooker);
  const online = useAppStore((s) => s.online);

  const pending = tracker?.pending_count ?? 0;
  const queued = tracker?.queued_count ?? 0;

  return (
    <aside className="w-60 shrink-0 bg-neutral text-neutral-content flex flex-col">
      <div className="h-14 flex items-center px-4 border-b border-neutral-content/10">
        <span className="font-bold tracking-tight">NihongoTracker</span>
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        <ul className="menu menu-sm w-full gap-1">
          {NAV.map(({ to, labelKey, icon: Icon, end, badge }) => (
            <li key={to}>
              <NavLink
                to={to}
                end={end}
                className={({ isActive }) =>
                  `rounded-field font-medium transition-all duration-200 ${
                    isActive
                      ? 'bg-primary text-primary-content'
                      : 'hover:bg-neutral-content/10'
                  }`
                }
              >
                <Icon className="w-4 h-4" />
                <span className="flex-1">{t(labelKey)}</span>
                {badge === 'pending' && pending > 0 ? (
                  <span className="badge badge-sm badge-warning">{pending}</span>
                ) : null}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>

      <div className="p-3 border-t border-neutral-content/10 space-y-2 text-xs">
        <div className="flex items-center gap-2">
          {online ? (
            <Cloud className="w-4 h-4 text-success" />
          ) : (
            <CloudOff className="w-4 h-4 text-warning" />
          )}
          <span className="opacity-80">
            {online === null
              ? t('status.checking')
              : online
                ? t('status.online')
                : t('status.offline')}
          </span>
          {queued > 0 ? (
            <span className="badge badge-xs badge-warning ml-auto">{queued}</span>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          <Plug
            className={`w-4 h-4 ${
              hooker?.status === 'connected'
                ? 'text-success'
                : hooker?.status === 'connecting'
                  ? 'text-warning'
                  : 'text-base-content/40'
            }`}
          />
          <span className="opacity-80 truncate">
            {hooker?.status === 'connected'
              ? t('status.hookerConnected')
              : hooker?.status === 'connecting'
                ? t('status.hookerConnecting')
                : t('status.hookerOffline')}
          </span>
        </div>
      </div>
    </aside>
  );
}
