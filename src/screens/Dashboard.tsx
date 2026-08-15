import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Link } from 'react-router-dom';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  AlertTriangle,
  BookOpenText,
  Check,
  Clock,
  Film,
  Flame,
  Inbox,
  KeyRound,
  MonitorPlay,
  Pause,
  Plus,
  RefreshCw,
  Type,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Page, SectionTitle, StatTile } from '../components/ui/Page';
import { getMediaTypeClasses } from '../constants/mediaColors';
import {
  formatClock,
  formatDuration,
  logTypeLabel,
  numberWithCommas,
  readingSpeed,
  relativeDate,
} from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { AnimeSession, VnSession } from '../lib/types';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { useQuickLogStore } from '../store/useQuickLogStore';

export default function Dashboard() {
  const { t } = useTranslation('dashboard');
  const tracker = useAppStore((s) => s.tracker);
  const settings = useAppStore((s) => s.settings);
  const online = useAppStore((s) => s.online);
  const user = useAppStore((s) => s.user);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const authRequired = useAppStore((s) => s.authRequired);
  const openQuickLog = useQuickLogStore((s) => s.openForm);
  const queryClient = useQueryClient();
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    const offOutbox = listen(events.outbox, () => {
      void queryClient.invalidateQueries({ queryKey: ['recent-logs'] });
      void queryClient.invalidateQueries({ queryKey: ['streak'] });
    });
    // Rust fires this at most once a day; the banner mirrors it in-app.
    const offStreak = listen(events.streakReminder, () => {
      void queryClient.invalidateQueries({ queryKey: ['streak'] });
    });
    return () => {
      void offOutbox.then((fn) => fn());
      void offStreak.then((fn) => fn());
    };
  }, [queryClient]);

  async function syncNow() {
    setSyncing(true);
    try {
      await ipc.flushOutbox();
      await refreshConnection();
    } finally {
      setSyncing(false);
    }
  }

  const anime = tracker?.anime ?? null;
  const vn = tracker?.vn ?? null;
  const idle = tracker?.idle_seconds ?? 0;

  // Refetched whenever the outbox drains, so a fresh log shows up right away.
  const { data: recentLogs } = useQuery({
    queryKey: ['recent-logs'],
    queryFn: () => ipc.recentLogs(8),
    enabled: online === true,
    retry: false,
  });
  const recent = recentLogs ?? [];

  // The command shares the reminder loop's cache, so this is free most of the
  // time; the day key is derived in Rust, in the user's own timezone.
  const { data: streak } = useQuery({
    queryKey: ['streak'],
    queryFn: () => ipc.streakStatus(),
    enabled: online === true && settings?.streak_reminder !== false,
    refetchInterval: 5 * 60_000,
    retry: false,
  });

  const stats = user?.stats ?? null;
  // The API gives absolute XP thresholds, so the bar needs both ends.
  const levelProgress = (() => {
    const xp = stats?.userXp;
    const from = stats?.userXpToCurrentLevel;
    const to = stats?.userXpToNextLevel;
    if (xp === undefined || from === undefined || to === undefined || to <= from) return null;
    return Math.min(100, Math.max(0, Math.round(((xp - from) / (to - from)) * 100)));
  })();

  return (
    <Page
      title={t('title')}
      description={tracker?.platform_supported ? t('subtitle') : t('subtitleUnsupported')}
      actions={
        <>
          <Button variant="primary" size="sm" onClick={() => openQuickLog('manual')}>
            <Plus className="w-4 h-4" />
            {t('logNow')}
          </Button>
          <Button appearance="outline" size="sm" loading={syncing} onClick={() => void syncNow()}>
            <RefreshCw className="w-4 h-4" />
            {t('sync')}
          </Button>
        </>
      }
    >
      {authRequired ? (
        <div className="alert alert-error">
          <KeyRound className="w-5 h-5" />
          <span className="text-sm">{t('authRequired')}</span>
          <Link to="/settings" className="btn btn-sm">
            {t('authRequiredAction')}
          </Link>
        </div>
      ) : null}

      {streak?.atRisk ? (
        <div className="alert alert-warning">
          <Flame className="w-5 h-5" />
          <span className="text-sm">
            {t('streakAtRisk', {
              count: streak.currentStreak,
              hours: Math.floor(streak.minutesLeft / 60),
              minutes: streak.minutesLeft % 60,
            })}
          </span>
          <Button size="sm" onClick={() => openQuickLog('streak')}>
            {t('logNow')}
          </Button>
        </div>
      ) : null}

      {online === false ? (
        <div className="alert alert-warning">
          <AlertTriangle className="w-5 h-5" />
          <span className="text-sm">
            {tracker?.queued_count
              ? t('offlineWarningQueued', { count: tracker.queued_count })
              : t('offlineWarning')}
          </span>
        </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        {anime ? <AnimeCard session={anime} /> : <IdleCard kind="anime" />}
        {vn ? <VnCard session={vn} idleSeconds={idle} /> : <IdleCard kind="vn" />}
      </div>

      {stats ? (
        <>
          <SectionTitle>{t('progress.title')}</SectionTitle>
          <div className="grid gap-3 grid-cols-2 lg:grid-cols-4">
            <StatTile
              label={t('progress.level')}
              value={stats.userLevel ?? 0}
              hint={
                levelProgress !== null
                  ? t('progress.toNextLevel', { percent: levelProgress })
                  : undefined
              }
              accent="text-primary"
            />
            <StatTile
              label={t('progress.streak')}
              value={t('progress.streakDays', { count: stats.currentStreak ?? 0 })}
              hint={t('progress.longestStreak', { count: stats.longestStreak ?? 0 })}
              accent="text-secondary"
            />
            <StatTile
              label={t('progress.reading')}
              value={stats.readingLevel ?? 0}
              hint={t('progress.levelUnit')}
              accent="text-accent"
            />
            <StatTile
              label={t('progress.listening')}
              value={stats.listeningLevel ?? 0}
              hint={t('progress.levelUnit')}
              accent="text-info"
            />
          </div>
        </>
      ) : null}

      <SectionTitle>{t('summary.title')}</SectionTitle>
      <div className="grid gap-3 grid-cols-2 lg:grid-cols-4">
        <StatTile
          label={t('summary.pending')}
          value={tracker?.pending_count ?? 0}
          hint={t('summary.pendingHint')}
          accent="text-warning"
        />
        <StatTile
          label={t('summary.queued')}
          value={tracker?.queued_count ?? 0}
          hint={t('summary.queuedHint')}
          accent="text-info"
        />
        <StatTile
          label={t('summary.idle')}
          value={formatDuration(idle)}
          hint={t('summary.idleHint', { seconds: settings?.afk_timeout_secs ?? 0 })}
          accent="text-base-content"
        />
        <StatTile
          label={t('summary.activeWindow')}
          value={<span className="text-sm font-medium">{tracker?.active_process ?? '—'}</span>}
          hint={tracker?.active_window ?? undefined}
          accent="text-base-content"
        />
      </div>

      {recent.length > 0 ? (
        <>
          <SectionTitle
            aside={
              <button
                type="button"
                className="btn btn-ghost btn-xs"
                onClick={() =>
                  void openUrl(
                    `${settings?.api_base_url ?? ''}/user/${settings?.username ?? ''}`
                  )
                }
              >
                {t('recent.openWeb')}
              </button>
            }
          >
            {t('recent.title')}
          </SectionTitle>
          <ul className="space-y-1.5">
            {recent.map((log) => {
              const logType = getMediaTypeClasses(log.type);
              return (
                <li
                  key={log._id}
                  className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-100/60"
                >
                  <span className={`w-1.5 h-8 rounded-full shrink-0 ${logType.accentColor}`} />
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-medium truncate">
                      {log.description}
                    </span>
                    <span className="block text-xs text-base-content/55">
                      {logTypeLabel(log.type)}
                      {log.episodes ? ` · ${log.episodes} ep.` : ''}
                      {log.time ? ` · ${log.time} min` : ''}
                      {log.chars ? ` · ${numberWithCommas(log.chars)} car.` : ''}
                    </span>
                  </span>
                  <span className={`badge badge-outline badge-sm ${logType.color} shrink-0`}>
                    {log.xp} XP
                  </span>
                  <span className="text-xs text-base-content/50 shrink-0 w-20 text-right">
                    {relativeDate(log.date)}
                  </span>
                </li>
              );
            })}
          </ul>
        </>
      ) : null}

      {(tracker?.pending_count ?? 0) > 0 ? (
        <Link to="/queue" className="btn btn-warning btn-block">
          <Inbox className="w-4 h-4" />
          {t('reviewPending', { count: tracker?.pending_count ?? 0 })}
        </Link>
      ) : null}
    </Page>
  );
}

function IdleCard({ kind }: { kind: 'anime' | 'vn' }) {
  const { t } = useTranslation('dashboard');
  const isAnime = kind === 'anime';
  return (
    <div className="surface overflow-hidden">
      <div className={`h-1 w-full ${getMediaTypeClasses(isAnime ? 'anime' : 'vn').accentColor}`} />
      <EmptyState
        icon={isAnime ? MonitorPlay : BookOpenText}
        title={isAnime ? t('anime.empty') : t('vn.empty')}
        description={isAnime ? t('anime.emptyBody') : t('vn.emptyBody')}
        action={
          !isAnime ? (
            <Link to="/library" className="btn btn-primary btn-sm">
              {t('vn.add')}
            </Link>
          ) : undefined
        }
      />
    </div>
  );
}

function AnimeCard({ session }: { session: AnimeSession }) {
  const { t } = useTranslation('dashboard');
  const type = getMediaTypeClasses('anime');
  const [matching, setMatching] = useState(false);
  const [logging, setLogging] = useState(false);
  const progress = Math.round(session.progress * 100);
  const fromPlayer = session.source === 'player';

  const title = session.media?.title ?? session.parsed.title;

  return (
    <article className={`surface overflow-hidden border ${type.borderColor}`}>
      <div className={`h-1 w-full ${type.accentColor}`} />
      <div className="p-4 space-y-3">
        <header className="flex items-start gap-3">
          {session.media?.image ? (
            <img
              src={session.media.image}
              alt=""
              className="w-14 h-20 object-cover rounded-field shrink-0"
            />
          ) : (
            <div className="w-14 h-20 rounded-field bg-base-200 flex items-center justify-center shrink-0">
              <Film className="w-5 h-5 text-base-content/40" />
            </div>
          )}
          <div className="min-w-0 flex-1">
            <div className={`badge badge-outline ${type.color} gap-1`}>
              <MonitorPlay className="w-3 h-3" />
              <span className="text-xs font-medium">{session.player}</span>
            </div>
            <h3 className="font-bold text-base leading-tight mt-1.5 truncate">{title}</h3>
            <p className="text-xs text-base-content/60 truncate">{session.file_name}</p>
          </div>
        </header>

        <div className="flex flex-wrap gap-2">
          {session.parsed.episode !== null ? (
            <span className={`badge badge-soft gap-1 ${type.bgColor} ${type.color}`}>
              {session.parsed.episode_end
                ? t('anime.episodeRange', {
                    from: session.parsed.episode,
                    to: session.parsed.episode_end,
                  })
                : t('anime.episode', { episode: session.parsed.episode })}
            </span>
          ) : null}
          {session.parsed.season ? (
            <span className="badge badge-soft gap-1">
              {t('anime.season', { season: session.parsed.season })}
            </span>
          ) : null}
          <span className="badge badge-ghost gap-1">
            <Clock className="w-3 h-3" />
            {fromPlayer && session.position_secs !== null && session.duration_secs !== null
              ? `${formatClock(session.position_secs)} / ${formatClock(session.duration_secs)}`
              : formatClock(session.elapsed_secs)}
          </span>
          {session.paused ? (
            <span className="badge badge-ghost gap-1">
              <Pause className="w-3 h-3" />
              {t('anime.paused')}
            </span>
          ) : null}
        </div>

        {!session.resolved ? (
          <p className="text-xs text-base-content/60">{t('anime.resolving')}</p>
        ) : null}

        {session.unmatched ? (
          <div className="alert alert-warning py-2">
            <AlertTriangle className="w-4 h-4" />
            <span className="text-xs">{t('anime.unmatched')}</span>
            <Button size="xs" appearance="outline" onClick={() => setMatching(true)}>
              {t('anime.link')}
            </Button>
          </div>
        ) : null}

        <div>
          <div className="flex justify-between text-xs text-base-content/60 mb-1">
            <span>
              {session.logged
                ? t('anime.logged')
                : fromPlayer
                  ? t('anime.progressPlayer', { percent: session.threshold_percent })
                  : t('anime.progressElapsed')}
            </span>
            <span>{progress}%</span>
          </div>
          <progress
            className={`progress w-full ${session.logged ? 'progress-success' : 'progress-info'}`}
            value={progress}
            max={100}
          />
        </div>

        {!session.logged ? (
          <Button
            size="sm"
            appearance="outline"
            block
            loading={logging}
            onClick={async () => {
              setLogging(true);
              try {
                await ipc.logCurrentAnime();
              } finally {
                setLogging(false);
              }
            }}
          >
            <Check className="w-4 h-4" />
            {t('anime.logNow')}
          </Button>
        ) : null}
      </div>

      <MediaSearchModal
        open={matching}
        onClose={() => setMatching(false)}
        mediaType="anime"
        initialQuery={session.parsed.title}
        onPick={async (hit) => {
          await ipc.setTitleMapping(session.parsed.title, {
            content_id: hit.contentId,
            title: hit.title.contentTitleRomaji || hit.title.contentTitleNative,
            media_type: hit.type ?? 'anime',
            image: hit.contentImage ?? hit.coverImage,
            episode_duration: hit.episodeDuration ?? null,
            is_adult: hit.isAdult,
            manual: true,
          });
          setMatching(false);
        }}
      />
    </article>
  );
}

function VnCard({ session, idleSeconds }: { session: VnSession; idleSeconds: number }) {
  const { t } = useTranslation('dashboard');
  const type = getMediaTypeClasses(session.media_type);
  const [, force] = useState(0);

  // The Rust side pushes a snapshot every second, but re-render anyway so the
  // clock never looks frozen if a tick is missed.
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const speed = readingSpeed(session.session_chars, session.elapsed_secs);

  return (
    <article className={`surface overflow-hidden border ${type.borderColor}`}>
      <div className={`h-1 w-full ${type.accentColor}`} />
      <div className="p-4 space-y-3">
        <header className="flex items-start gap-3">
          {session.image ? (
            <img
              src={session.image}
              alt=""
              className="w-14 h-20 object-cover rounded-field shrink-0"
            />
          ) : (
            <div className="w-14 h-20 rounded-field bg-base-200 flex items-center justify-center shrink-0">
              <BookOpenText className="w-5 h-5 text-base-content/40" />
            </div>
          )}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span
                className={`w-2 h-2 rounded-full ${
                  session.counting ? 'bg-success recording-dot' : 'bg-base-content/30'
                }`}
              />
              <span className="text-xs font-medium text-base-content/70">
                {session.counting ? t('vn.counting') : t('vn.autoPaused')}
              </span>
            </div>
            <h3 className="font-bold text-base leading-tight mt-1 truncate">
              {session.media_title}
            </h3>
            <p className="text-xs text-base-content/60 truncate">{session.exe}</p>
          </div>
        </header>

        <div className="grid grid-cols-3 gap-2 text-center">
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.time')}
            </div>
            <div className="font-bold tabular-nums">{formatClock(session.elapsed_secs)}</div>
          </div>
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.characters')}
            </div>
            <div className="font-bold tabular-nums">{numberWithCommas(session.session_chars)}</div>
          </div>
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.speed')}
            </div>
            <div className="font-bold tabular-nums">{numberWithCommas(speed)}</div>
          </div>
        </div>

        {!session.counting ? (
          <p className="text-xs text-base-content/60 flex items-center gap-1.5">
            <Pause className="w-3 h-3" />
            {idleSeconds > 0
              ? t('vn.idleFor', { duration: formatDuration(idleSeconds) })
              : t('vn.notFocused')}
          </p>
        ) : null}

        <Link to="/texthooker" className="btn btn-primary btn-sm w-full">
          <Type className="w-4 h-4" />
          {t('vn.openTexthooker')}
        </Link>
      </div>
    </article>
  );
}
