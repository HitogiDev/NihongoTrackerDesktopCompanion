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
  ChevronDown,
  Clock,
  Film,
  Flame,
  Headphones,
  Inbox,
  KeyRound,
  MonitorPlay,
  Pause,
  Play,
  Plus,
  RefreshCw,
  Type,
  Trophy,
  UsersRound,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState, ErrorState, ListSkeleton } from '../components/ui/Empty';
import { Page, StatTile } from '../components/ui/Page';
import { getMediaTypeClasses } from '../constants/mediaColors';
import {
  formatClock,
  formatDuration,
  formatMinutes,
  logTypeLabel,
  numberWithCommas,
  readingSpeed,
  relativeDate,
} from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { AnimeSession, UserSummary, VnSession } from '../lib/types';
import { MediaSearchModal } from '../components/MediaSearchModal';
import { MpvSetupPrompt, needsMpvSetup } from '../components/MpvSetup';
import { useQuickLogStore } from '../store/useQuickLogStore';
import { toast } from '../store/useToastStore';
import { SocialActivityFeed } from '../components/SocialActivityFeed';
import { FriendsImmersing } from '../components/FriendsImmersing';

function xpLevelProgress(xp?: number, from?: number, to?: number): number | null {
  // The API gives absolute XP thresholds, so the bar needs both ends.
  if (xp === undefined || from === undefined || to === undefined || to <= from) return null;
  return Math.min(100, Math.max(0, Math.round(((xp - from) / (to - from)) * 100)));
}

const COMMUNITY_KEY = 'dashboard:community-open';

function storedCommunityOpen(): boolean {
  try {
    return localStorage.getItem(COMMUNITY_KEY) !== 'false';
  } catch {
    return true;
  }
}

export default function Dashboard() {
  const { t } = useTranslation('dashboard');
  // Primitive selectors: the snapshot changes every second while a session
  // runs, and only `ActiveSessions` needs to follow it.
  const platformSupported = useAppStore((s) => s.tracker?.platform_supported ?? false);
  const fullDetection = useAppStore((s) =>
    Boolean(s.tracker?.capabilities.window_titles && s.tracker.capabilities.foreground));
  const queuedCount = useAppStore((s) => s.tracker?.queued_count ?? 0);
  const pendingCount = useAppStore((s) => s.tracker?.pending_count ?? 0);
  const settings = useAppStore((s) => s.settings);
  const online = useAppStore((s) => s.online);
  const stats = useAppStore((s) => s.user?.stats ?? null);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const authRequired = useAppStore((s) => s.authRequired);
  const openQuickLog = useQuickLogStore((s) => s.openForm);
  const queryClient = useQueryClient();
  const [syncing, setSyncing] = useState(false);

  useEffect(() => {
    const offOutbox = listen(events.outbox, () => {
      void queryClient.invalidateQueries({ queryKey: ['recent-logs'] });
      void queryClient.invalidateQueries({ queryKey: ['streak'] });
      void queryClient.invalidateQueries({ queryKey: ['today-summary'] });
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
      if (useAppStore.getState().online === false) toast.warning(t('toast.syncOffline'));
      else toast.success(t('toast.synced'));
    } catch (error) {
      toast.error(t('toast.syncFailed', { message: String(error) }));
    } finally {
      setSyncing(false);
    }
  }

  // The command shares the reminder loop's cache, so this is free most of the
  // time; the day key is derived in Rust, in the user's own timezone.
  const { data: streak } = useQuery({
    queryKey: ['streak'],
    queryFn: () => ipc.streakStatus(),
    enabled: online === true && settings?.streak_reminder !== false,
    refetchInterval: 5 * 60_000,
    retry: false,
  });

  return (
    <Page
      title={t('title')}
      description={!platformSupported
        ? t('subtitleUnsupported')
        : fullDetection ? t('subtitle') : t('subtitlePartial')}
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
          <button type="button" onClick={() => void ipc.openAuxWindow('settings')} className="btn btn-sm">
            {t('authRequiredAction')}
          </button>
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
            {queuedCount
              ? t('offlineWarningQueued', { count: queuedCount })
              : t('offlineWarning')}
          </span>
        </div>
      ) : null}

      {pendingCount > 0 ? (
        <div className="alert alert-info alert-soft">
          <Inbox className="w-5 h-5" />
          <span className="text-sm">{t('pendingNotice', { count: pendingCount })}</span>
          <Link to="/queue" className="btn btn-sm">
            {t('pendingReview')}
          </Link>
        </div>
      ) : null}

      <ActiveSessions />
      <TodaySection />
      <RecentLogs />
      {stats ? <ProgressSection stats={stats} /> : null}
      <CommunitySection />
    </Page>
  );
}

/** Minutes, characters and episodes logged from this computer today. The day
 *  is the account's, decided in Rust; logs from the web are not in the outbox. */
function TodaySection() {
  const { t } = useTranslation('dashboard');
  const { data: today } = useQuery({
    queryKey: ['today-summary'],
    queryFn: () => ipc.todaySummary(),
    // The day can turn over while the window is open.
    refetchInterval: 5 * 60_000,
    retry: false,
  });
  // Until the server has answered, the account's own numbers stand in.
  const stats = useAppStore((s) => s.user?.stats);
  const streakDays = today?.streak ?? stats?.currentStreak ?? null;
  const longest = stats?.longestStreak ?? null;

  return (
    <section className="@container space-y-3" aria-labelledby="dashboard-today">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <h2 id="dashboard-today" className="text-base font-semibold">{t('today.title')}</h2>
        <span className="text-xs text-base-content/60">{t('today.scope')}</span>
      </div>
      {/* Lines between the tiles are the grid gap showing through. */}
      <div className="surface overflow-hidden">
        <div className="grid grid-cols-2 gap-px bg-base-300/50 @2xl:grid-cols-4">
          <div className="bg-base-100"><StatTile compact icon={Clock} label={t('today.time')} value={today ? formatMinutes(today.minutes) : '—'} /></div>
          <div className="bg-base-100"><StatTile compact icon={Type} label={t('today.chars')} value={today ? numberWithCommas(today.chars) : '—'} /></div>
          <div className="bg-base-100"><StatTile compact icon={MonitorPlay} label={t('today.episodes')} value={today ? today.episodes : '—'} /></div>
          <div className="bg-base-100"><StatTile
            compact
            icon={Flame}
            label={t('today.streak')}
            value={streakDays != null ? t('progress.streakDays', { count: streakDays }) : '—'}
            hint={today?.streak != null
              ? today.streak_today ? t('today.streakCounted') : t('today.streakPending')
              : longest != null ? t('progress.longestStreak', { count: longest }) : undefined}
            accent={today?.streak_today ? 'text-success' : 'text-base-content'}
          /></div>
        </div>
      </div>
      <p className="text-xs text-base-content/55">{t('today.webHint')}</p>
    </section>
  );
}

function RecentLogs() {
  const { t } = useTranslation('dashboard');
  const { t: tCommon } = useTranslation('common');
  const online = useAppStore((s) => s.online);
  const settings = useAppStore((s) => s.settings);
  // Refetched whenever the outbox drains, so a fresh log shows up right away.
  const recentLogs = useQuery({
    queryKey: ['recent-logs'],
    queryFn: () => ipc.recentLogs(8),
    enabled: online === true,
    retry: false,
  });
  const recent = recentLogs.data ?? [];
  // Offline the query is off and the notice above already says why.
  if (online !== true && recent.length === 0) return null;

  return (
    <section className="space-y-3" aria-labelledby="dashboard-recent">
      <div className="flex items-center justify-between gap-3">
        <h2 id="dashboard-recent" className="text-base font-semibold">{t('recent.title')}</h2>
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
      </div>
      {recentLogs.isLoading ? <ListSkeleton rows={3} className="h-12" /> : null}
      {recentLogs.isError && recent.length === 0 ? (
        <div className="surface">
          <ErrorState
            compact
            title={t('recent.loadFailed')}
            error={recentLogs.error}
            retrying={recentLogs.isFetching}
            onRetry={() => void recentLogs.refetch()}
          />
        </div>
      ) : null}
      {recentLogs.isSuccess && recent.length === 0 ? (
        <div className="surface">
          <EmptyState compact icon={Inbox} title={t('recent.empty')} description={t('recent.emptyBody')} />
        </div>
      ) : null}
      <ul className="space-y-1.5">
        {recent.map((log) => {
          const logType = getMediaTypeClasses(log.type);
          const title = log.mediaTitle || log.media?.title?.contentTitleRomaji || log.media?.title?.contentTitleEnglish || log.media?.title?.contentTitleNative || log.description;
          return (
            <li
              key={log._id}
              className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-100/60"
            >
              <span className={`w-1.5 h-8 rounded-full shrink-0 ${logType.accentColor}`} />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium truncate">
                  {title}
                </span>
                {log.description && log.description !== title ? <span className="block text-xs text-base-content/70 truncate">{log.description}</span> : null}
                <span className="block text-xs text-base-content/55">
                  {logTypeLabel(log.type)}
                  {log.episodes ? ` · ${tCommon('episodesShort', { count: log.episodes })}` : ''}
                  {log.time ? ` · ${tCommon('minutesShort', { count: log.time })}` : ''}
                  {log.chars ? ` · ${tCommon('charsShort', { count: log.chars })}` : ''}
                </span>
              </span>
              <span className={`badge badge-outline badge-sm ${logType.color} shrink-0`}>
                {tCommon('xpValue', { value: log.xp })}
              </span>
              <span className="text-xs text-base-content/50 shrink-0 w-20 text-right">
                {relativeDate(log.date)}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function ProgressSection({ stats }: { stats: NonNullable<UserSummary['stats']> }) {
  const { t } = useTranslation('dashboard');
  const levelProgress = xpLevelProgress(
    stats.userXp, stats.userXpToCurrentLevel, stats.userXpToNextLevel,
  );
  const readingProgress = xpLevelProgress(
    stats.readingXp, stats.readingXpToCurrentLevel, stats.readingXpToNextLevel,
  );
  const listeningProgress = xpLevelProgress(
    stats.listeningXp, stats.listeningXpToCurrentLevel, stats.listeningXpToNextLevel,
  );
  return (
    <section className="@container space-y-3" aria-labelledby="dashboard-progress">
      <h2 id="dashboard-progress" className="text-base font-semibold">{t('progress.title')}</h2>
      <div className="grid gap-3 @sm:grid-cols-3">
        <StatTile
          icon={Trophy}
          label={t('progress.level')}
          value={stats.userLevel ?? 0}
          progress={levelProgress}
          hint={
            levelProgress !== null
              ? t('progress.toNextLevel', { percent: levelProgress })
              : undefined
          }
          accent="text-primary"
        />
        <StatTile
          icon={BookOpenText}
          label={t('progress.reading')}
          value={stats.readingLevel ?? 0}
          progress={readingProgress}
          hint={readingProgress !== null
            ? t('progress.toNextLevel', { percent: readingProgress })
            : undefined}
        />
        <StatTile
          icon={Headphones}
          label={t('progress.listening')}
          value={stats.listeningLevel ?? 0}
          progress={listeningProgress}
          hint={listeningProgress !== null
            ? t('progress.toNextLevel', { percent: listeningProgress })
            : undefined}
        />
      </div>
    </section>
  );
}

/** Friends and the activity feed, folded away on request. While folded they
 *  are not mounted, so their queries stop polling too. */
function CommunitySection() {
  const { t } = useTranslation('dashboard');
  const [open, setOpen] = useState(storedCommunityOpen);

  const toggle = () => {
    const next = !open;
    setOpen(next);
    try {
      localStorage.setItem(COMMUNITY_KEY, String(next));
    } catch {
      // Without storage it simply opens again next time.
    }
  };

  return (
    <section className="space-y-4" aria-labelledby="dashboard-community">
      <button
        type="button"
        className="flex w-full items-center gap-2 rounded-field py-1 text-left"
        aria-expanded={open}
        aria-controls="dashboard-community-body"
        onClick={toggle}
      >
        <UsersRound className="h-4 w-4 text-base-content/60" />
        <h2 id="dashboard-community" className="text-base font-semibold">{t('community.title')}</h2>
        <span className="ml-auto text-xs text-base-content/60">
          {open ? t('community.hide') : t('community.show')}
        </span>
        <ChevronDown className={open ? 'h-4 w-4 rotate-180 text-base-content/60 motion-safe:transition-transform' : 'h-4 w-4 text-base-content/60 motion-safe:transition-transform'} />
      </button>
      {open ? (
        <div id="dashboard-community-body" className="space-y-6">
          <FriendsImmersing />
          <SocialActivityFeed compact />
        </div>
      ) : null}
    </section>
  );
}

/** The only part of the Dashboard that follows the per-second snapshot. */
function ActiveSessions() {
  const anime = useAppStore((s) => s.tracker?.anime ?? null);
  const vn = useAppStore((s) => s.tracker?.vn ?? null);
  const idle = useAppStore((s) => s.tracker?.idle_seconds ?? 0);

  // Nothing running: two lines, so today's numbers stay near the top.
  if (!anime && !vn) {
    return (
      <div className="grid gap-2 lg:grid-cols-2">
        <IdleCard kind="anime" />
        <IdleCard kind="vn" />
      </div>
    );
  }
  // What is running comes first; what isn't shrinks to a line under it.
  return (
    <div className="space-y-2">
      <div className={`grid gap-4 ${anime && vn ? 'lg:grid-cols-2' : ''}`}>
        {anime ? <AnimeCard key={anime.key} session={anime} /> : null}
        {vn ? <VnCard session={vn} idleSeconds={idle} /> : null}
      </div>
      {!anime ? <IdleCard kind="anime" /> : null}
      {!vn ? <IdleCard kind="vn" /> : null}
    </div>
  );
}

function IdleCard({ kind }: { kind: 'anime' | 'vn' }) {
  const { t } = useTranslation('dashboard');
  const isAnime = kind === 'anime';
  const Icon = isAnime ? MonitorPlay : BookOpenText;
  return (
    <div className="surface-muted flex items-center gap-3 px-4 py-2.5">
      <Icon className="w-4 h-4 shrink-0 text-base-content/50" />
      <p className="min-w-0 flex-1 text-sm">
        <span className="font-medium">{isAnime ? t('anime.empty') : t('vn.empty')}</span>
        <span className="text-base-content/60"> · {isAnime ? t('anime.emptyBody') : t('vn.emptyBody')}</span>
      </p>
      {!isAnime ? (
        <Link to="/library" className="btn btn-ghost btn-xs shrink-0">
          {t('vn.add')}
        </Link>
      ) : null}
    </div>
  );
}

function AnimeCard({ session }: { session: AnimeSession }) {
  const { t } = useTranslation('dashboard');
  const type = getMediaTypeClasses('anime');
  const [matching, setMatching] = useState(false);
  const [logging, setLogging] = useState(false);
  const [comment, setComment] = useState('');
  const progress = Math.round(session.progress * 100);
  const fromPlayer = session.source === 'player';
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

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

        {needsMpvSetup(session, settings) ? (
          <MpvSetupPrompt player={session.player} onDismiss={() => void updateSettings({ mpv_setup_dismissed: true })} />
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
              {session.logged && session.pending ? (
                <Link to="/queue" className="link link-warning">
                  {t('anime.sentToPending')}
                </Link>
              ) : session.logged
                ? t('anime.logged')
                : fromPlayer
                  ? t('anime.progressPlayer', { percent: session.threshold_percent })
                  : t('anime.progressElapsed')}
            </span>
            <span>{progress}%</span>
          </div>
          <progress
            className={`progress w-full ${
              session.pending ? 'progress-warning' : session.logged ? 'progress-success' : 'progress-info'
            }`}
            value={progress}
            max={100}
          />
        </div>

        {!session.logged ? (
          <label className="block space-y-1 text-sm">
            <span>{t('anime.comment')}</span>
            <textarea
              className="textarea w-full"
              rows={2}
              value={comment}
              onChange={(event) => setComment(event.target.value)}
              placeholder={t('anime.commentPlaceholder')}
            />
          </label>
        ) : null}

        {!session.logged ? (
          <Button
            size="sm"
            appearance="outline"
            block
            loading={logging}
            onClick={async () => {
              setLogging(true);
              try {
                await ipc.logCurrentAnime(comment.trim());
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
  const gameHook = useAppStore((s) => s.tracker?.game_hook ?? null);
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

        {(session.media_type === 'game' || session.media_type === 'vn') && gameHook && gameHook.phase !== 'inactive' ? (
          <div className="rounded-field bg-base-200/60 px-3 py-2">
            <p className="text-xs font-medium">
              {gameHook.phase === 'starting'
                ? t('vn.nativeHookStarting')
                : gameHook.phase === 'attached'
                  ? t('vn.nativeHookAttached')
                  : gameHook.phase === 'receiving'
                    ? t('vn.nativeHookReceiving')
                    : gameHook.phase === 'unavailable'
                      ? t('vn.nativeHookUnavailable')
                      : t('vn.nativeHookError')}
            </p>
            {gameHook.message ? (
              <p className="mt-1 text-[11px] text-base-content/60">{gameHook.message}</p>
            ) : null}
          </div>
        ) : null}

        <div className={`grid gap-2 text-center ${session.media_type === 'game' ? 'grid-cols-1' : 'grid-cols-3'}`}>
          <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.time')}
            </div>
            <div className="font-bold tabular-nums">{formatClock(session.elapsed_secs)}</div>
          </div>
          {session.media_type !== 'game' ? <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.characters')}
            </div>
            <div className="font-bold tabular-nums">{numberWithCommas(session.session_chars)}</div>
          </div> : null}
          {session.media_type !== 'game' ? <div className="rounded-field bg-base-200/60 py-2">
            <div className="text-[10px] uppercase tracking-wide text-base-content/60">
              {t('vn.speed')}
            </div>
            <div className="font-bold tabular-nums">{numberWithCommas(speed)}</div>
          </div> : null}
        </div>

        {!session.counting ? (
          <p className="text-xs text-base-content/60 flex items-center gap-1.5">
            <Pause className="w-3 h-3" />
            {session.paused_by_user
              ? t('vn.manuallyPaused')
              : idleSeconds > 0
              ? t('vn.idleFor', { duration: formatDuration(idleSeconds) })
              : t('vn.notFocused')}
          </p>
        ) : null}

        <div className="flex gap-2">
          <button type="button" onClick={() => void ipc.setSessionPaused(!session.paused_by_user)} className="btn btn-outline btn-sm">
            {session.paused_by_user ? <Play className="w-4 h-4" /> : <Pause className="w-4 h-4" />}
            {session.paused_by_user ? t('vn.resume') : t('vn.pause')}
          </button>
          <button type="button" onClick={() => void ipc.openWorkTexthooker(session.content_id)} className="btn btn-primary btn-sm flex-1">
            <Type className="w-4 h-4" />
            {t('vn.openTexthooker')}
          </button>
        </div>
      </div>
    </article>
  );
}
