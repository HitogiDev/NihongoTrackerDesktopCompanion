import { useInfiniteQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Award, BookOpen, Film, Flame, Globe2, UsersRound } from 'lucide-react';
import { EmptyState } from './ui/Empty';
import { Spinner } from './ui/Spinner';
import { numberWithCommas, relativeDate } from '../lib/format';
import { ipc } from '../lib/ipc';
import type { SocialActivity } from '../lib/types';
import { useAppStore } from '../store/useAppStore';

type FeedScope = 'following' | 'clubs' | 'global';

const SCOPE_OPTIONS: Array<{ value: FeedScope; icon: typeof UsersRound }> = [
  { value: 'following', icon: UsersRound },
  { value: 'clubs', icon: UsersRound },
  { value: 'global', icon: Globe2 },
];

function metadataText(activity: SocialActivity, key: string) {
  const value = activity.metadata?.[key];
  return typeof value === 'string' ? value : '';
}

function metadataNumber(activity: SocialActivity, key: string) {
  const value = activity.metadata?.[key];
  return typeof value === 'number' ? value : 0;
}

function activityIcon(type: string) {
  if (type.includes('achievement') || type.includes('completed')) return Award;
  if (type.includes('session')) return Flame;
  if (type === 'media_started' || type === 'media_completed') return Film;
  return BookOpen;
}

function ActivityRow({ activity }: { activity: SocialActivity }) {
  const { t } = useTranslation('social');
  const settings = useAppStore((s) => s.settings);
  const Icon = activityIcon(activity.type);
  const subject = metadataText(activity, 'mediaTitle') ||
    metadataText(activity, 'clubName') ||
    metadataText(activity, 'challengeTitle') ||
    metadataText(activity, 'goalTitle') ||
    metadataText(activity, 'name') ||
    metadataText(activity, 'description');
  const metrics = [
    metadataNumber(activity, 'chars') ? `${numberWithCommas(metadataNumber(activity, 'chars'))} ${t('metrics.characters')}` : '',
    metadataNumber(activity, 'time') ? `${metadataNumber(activity, 'time')} ${t('metrics.minutes')}` : '',
    metadataNumber(activity, 'episodes') ? `${metadataNumber(activity, 'episodes')} ${t('metrics.episodes')}` : '',
    metadataNumber(activity, 'pages') ? `${metadataNumber(activity, 'pages')} ${t('metrics.pages')}` : '',
  ].filter(Boolean);

  return (
    <article className="flex gap-3 border-b border-base-300/60 px-4 py-4 last:border-0 sm:px-5">
      <button
        type="button"
        className="h-9 w-9 shrink-0 overflow-hidden rounded-full bg-base-200 text-sm font-semibold"
        title={t('openProfile', { username: activity.actor.username })}
        onClick={() => void openUrl(`${settings?.api_base_url ?? ''}/user/${encodeURIComponent(activity.actor.username)}`).catch(() => undefined)}
      >
        {activity.actor.avatar ? <img src={activity.actor.avatar} alt="" className="h-full w-full object-cover" /> : <span className="grid h-full w-full place-items-center">{activity.actor.username.slice(0, 1).toUpperCase()}</span>}
      </button>
      <div className="min-w-0 flex-1">
        <header className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
          <button type="button" className="truncate text-sm font-semibold hover:text-primary" onClick={() => void openUrl(`${settings?.api_base_url ?? ''}/user/${encodeURIComponent(activity.actor.username)}`).catch(() => undefined)}>
            {activity.actor.username}
          </button>
          <span className="text-xs text-base-content/55">{relativeDate(activity.occurredAt)}</span>
        </header>
        <div className="mt-1.5 flex items-start gap-2 text-sm leading-relaxed">
          <Icon className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
          <p className="min-w-0 break-words text-base-content/70">
            {t(`activity.${activity.type}`, { defaultValue: t('activity.generic') })}
            {subject && <span className="font-medium text-base-content"> {subject}</span>}
          </p>
        </div>
        {metrics.length > 0 && <p className="ml-6 mt-1 text-xs text-base-content/55">{metrics.join(' · ')}</p>}
      </div>
    </article>
  );
}

export function SocialActivityFeed({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation('social');
  const online = useAppStore((s) => s.online);
  const [scope, setScope] = useState<FeedScope>('following');
  const limit = compact ? 6 : 20;
  const feed = useInfiniteQuery({
    queryKey: ['activity-feed', scope, limit],
    queryFn: ({ pageParam }) => ipc.activityFeed({ scope, before: pageParam, limit }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
    enabled: online === true,
    retry: false,
  });
  const activities = feed.data?.pages.flatMap((page) => page.activities) ?? [];

  return (
    <section aria-label={t('title')}>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{compact ? t('friendActivity') : t('title')}</h2>
          {!compact && <p className="mt-0.5 text-sm text-base-content/60">{t('subtitle')}</p>}
        </div>
        {compact ? (
          <Link to="/social" className="btn btn-ghost btn-sm">{t('openSocial')}</Link>
        ) : (
          <div role="tablist" aria-label={t('title')} className="join">
            {SCOPE_OPTIONS.map(({ value, icon: Icon }) => (
              <button key={value} type="button" role="tab" aria-selected={scope === value} className={`join-item btn btn-sm ${scope === value ? 'btn-primary' : 'btn-ghost'}`} onClick={() => setScope(value)}>
                <Icon className="h-4 w-4" />
                <span className="hidden sm:inline">{t(`scope.${value}`)}</span>
              </button>
            ))}
          </div>
        )}
      </header>

      {online === false ? (
        <div className="surface-muted"><EmptyState icon={Globe2} title={t('offline')} /></div>
      ) : feed.isPending ? (
        <div className="surface-muted flex min-h-40 items-center justify-center"><Spinner /></div>
      ) : feed.isError ? (
        <div role="alert" className="alert alert-error">
          <span>{t('loadError')}</span>
          <button type="button" className="btn btn-sm" onClick={() => void feed.refetch()}>{t('retry')}</button>
        </div>
      ) : activities.length === 0 ? (
        <div className="surface-muted"><EmptyState icon={UsersRound} title={t(`empty.${scope}`)} description={scope === 'following' ? t('followingHint') : undefined} /></div>
      ) : (
        <div className="overflow-hidden rounded-box border border-base-300/70 bg-base-100">
          {activities.map((activity) => <ActivityRow key={activity._id} activity={activity} />)}
        </div>
      )}

      {!compact && feed.hasNextPage && (
        <div className="mt-4 flex justify-center">
          <button type="button" className="btn btn-ghost btn-sm" disabled={feed.isFetchingNextPage} onClick={() => void feed.fetchNextPage()}>
            {feed.isFetchingNextPage ? <Spinner size="xs" /> : null}{t('loadMore')}
          </button>
        </div>
      )}
    </section>
  );
}
