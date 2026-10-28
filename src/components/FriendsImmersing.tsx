import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { UsersRound } from 'lucide-react';
import { EmptyState } from './ui/Empty';
import { Spinner } from './ui/Spinner';
import { ipc } from '../lib/ipc';
import { formatClock } from '../lib/format';
import { useAppStore } from '../store/useAppStore';

export function FriendsImmersing() {
  const { t } = useTranslation(['social', 'common']);
  const online = useAppStore((state) => state.online);
  const baseUrl = useAppStore((state) => state.settings?.api_base_url ?? 'https://nihongotracker.com');
  const query = useQuery({ queryKey: ['friends-immersing'], queryFn: ipc.friendsImmersionPresence, enabled: online === true, refetchInterval: 15_000, retry: false });
  const friends = query.data ?? [];
  return <section className="space-y-3">
    <header><h2 className="text-lg font-semibold">{t('social:immersing.title')}</h2><p className="text-sm text-base-content/60">{t('social:immersing.hint')}</p></header>
    {online !== true ? <div className="surface-muted"><EmptyState icon={UsersRound} title={t('social:offline')} /></div> : query.isPending ? <div className="surface-muted flex min-h-20 items-center justify-center"><Spinner /></div> : friends.length === 0 ? <div className="surface-muted"><EmptyState icon={UsersRound} title={t('social:immersing.empty')} /></div> : <ul className="grid gap-2 sm:grid-cols-2">{friends.map((friend) => <li key={friend.username} className="surface flex min-w-0 items-center gap-3 p-3">
      <button type="button" className="h-10 w-10 shrink-0 overflow-hidden rounded-full bg-base-200 ring-2 ring-primary/40" title={t('social:openProfile', { username: friend.username })} onClick={() => void openUrl(`${baseUrl}/user/${encodeURIComponent(friend.username)}`).catch(() => undefined)}>{friend.avatar ? <img src={friend.avatar} alt="" className="h-full w-full object-cover" /> : <span className="grid h-full w-full place-items-center font-semibold">{friend.username[0]?.toUpperCase()}</span>}</button>
      <div className="min-w-0 flex-1"><button type="button" className="block max-w-full truncate text-left font-semibold hover:text-primary" onClick={() => void openUrl(`${baseUrl}/user/${encodeURIComponent(friend.username)}`).catch(() => undefined)}>{friend.username}</button><p className="truncate text-sm">{friend.mediaTitle}</p><p className="text-xs text-base-content/55">{t('social:immersing.elapsed', { time: formatClock(friend.elapsedSeconds) })} · {t(`common:logType.${friend.mediaType}`, { defaultValue: friend.mediaType })}</p></div>
    </li>)}</ul>}
  </section>;
}
