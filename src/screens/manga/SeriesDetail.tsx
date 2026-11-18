import { useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, BookOpen, Puzzle, Check, ShieldAlert, CloudDownload, CloudOff, FileArchive, FileDown, Folder, HardDriveDownload, Link2, Play, RefreshCw, ScanText, Server, Trash2, Unlink, UploadCloud, X } from 'lucide-react';
import { MediaSearchModal } from '../../components/MediaSearchModal';
import { Button } from '../../components/ui/Button';
import DropdownSelect from '../../components/ui/DropdownSelect';
import { EmptyState, ErrorState, Skeleton } from '../../components/ui/Empty';
import { ipc } from '../../lib/ipc';
import { displayUrl, formatBytes, formatCount, mangaUrl, readPercent } from '../../lib/manga';
import type { DownloadProgress, MangaSeries, MangaSource, MangaVolume, OcrStatus } from '../../lib/types';
import { OcrInstallModal } from './ocr/OcrInstallModal';
import { OcrExportModal } from './ocr/OcrExportModal';
import { OcrVolumeLabel, OcrVolumeState } from './ocr/OcrVolumeState';
import { useOcrStatus } from './ocr/useOcr';
import { toast } from '../../store/useToastStore';
import { Cover, MANGA } from './Cover';
import { backToLibrary, COVER_TRANSITION } from './coverTransition';
import { inViewTransition } from '../../lib/viewTransition';
import { openVolume, useMangaLibraryEvents, useMangaSeries } from './useMangaLibrary';
import { extErrorText } from './ext/useExtensions';
import { SuggestionCard } from './Suggestions';
import { useSuggestions } from './useSuggestions';
import { downloadForOffline, syncSource, useDownloadEvents, useDownloadStore } from './useWebdav';

/** A chapter of a series from an extension (its pages come from the Suwayomi server). */
const isChapter = (volume: MangaVolume) => volume.format === 'suwayomi-chapter';
const isRemote = (volume: MangaVolume) => volume.format === 'remote-archive' || volume.format === 'remote-folder' || isChapter(volume);
/** Something can be opened: pages, or a remote archive whose pages are read on opening. */
const isReadable = (volume: MangaVolume) => volume.page_count > 0 || isRemote(volume);
/** Why the series' server cannot be read from right now, if it cannot. */
export type Unreachable = 'offline' | 'unauthorized' | null;
export function unreachableReason(source: MangaSource | null): Unreachable {
  const error = source?.last_error;
  return error === 'offline' || error === 'unauthorized' ? error : null;
}

/** The volume "Read" opens: the one in progress, else the first unread one. */
export function nextVolume(volumes: MangaVolume[]): { volume: MangaVolume; resume: boolean } | null {
  const readable = volumes.filter(isReadable);
  const started = readable
    .filter((volume) => volume.page !== null && !volume.finished)
    .sort((a, b) => (b.read_at ?? '').localeCompare(a.read_at ?? ''))[0];
  if (started) return { volume: started, resume: true };
  const unread = readable.find((volume) => !volume.finished);
  if (unread) return { volume: unread, resume: false };
  return readable[0] ? { volume: readable[0], resume: false } : null;
}

function Hero({ series, volumes, source, onLink }: { series: MangaSeries; volumes: MangaVolume[]; source: MangaSource | null; onLink: () => void }) {
  const { t } = useTranslation(['manga', 'common']);
  // Offline, or refused by the server, "Read" only offers what is on this computer.
  const cut = unreachableReason(source) !== null;
  const next = nextVolume(cut ? volumes.filter((volume) => volume.downloaded || !isRemote(volume)) : volumes);
  const cover = mangaUrl.cover(series.id);
  const percent = series.page_count > 0 ? Math.round((series.pages_read / series.page_count) * 100) : 0;
  const [unlinking, setUnlinking] = useState(false);

  const unlink = async () => {
    setUnlinking(true);
    try {
      await ipc.mangaLinkSeries(series.id, null);
    } catch (error) {
      toast.error(t('toast.linkFailed', { message: String(error) }));
    } finally {
      setUnlinking(false);
    }
  };

  return (
    <section className="surface relative isolate overflow-hidden">
      {/* The cover, blurred, behind everything: the page takes its colours. */}
      <img src={cover} alt="" aria-hidden="true" draggable={false} className="absolute inset-0 -z-10 h-full w-full scale-125 object-cover opacity-80 blur-3xl saturate-150" />
      <div className="absolute inset-0 -z-10 bg-linear-to-r from-base-100/95 via-base-100/70 to-base-100/25" />
      <div className="flex flex-col gap-6 p-6 sm:flex-row sm:items-end">
        <div data-hero-cover style={{ viewTransitionName: COVER_TRANSITION }} className="aspect-[2/3] w-36 shrink-0 overflow-hidden rounded-box shadow-lg ring-1 ring-base-content/10 sm:w-44">
          <Cover src={cover} eager />
        </div>
        <div className="min-w-0 flex-1 space-y-4">
          <div>
            <p className={`text-xs font-semibold uppercase tracking-[0.18em] ${MANGA.color}`}>{t('common:logType.manga')}</p>
            <h2 className="mt-1 text-2xl font-bold leading-tight sm:text-3xl">{series.title}</h2>
            {source?.kind === 'suwayomi' ? (
              <p className="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-base-content/55">
                <Puzzle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                <span className="truncate">{t('ext.detail.from', { server: source.id === 'suwayomi-managed' ? t('ext.server.managed') : displayUrl(source.url) })}</span>
              </p>
            ) : source ? (
              <p className="mt-1 flex min-w-0 items-center gap-1.5 text-xs text-base-content/55" title={`${source.url} ${series.location}`}>
                <Server className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                <span className="truncate">{source.name}</span>
                <span aria-hidden="true">·</span>
                <span className="selectable truncate font-mono text-[11px]">{displayUrl(source.url)}</span>
              </p>
            ) : (
              <p className="selectable mt-1 truncate font-mono text-[11px] text-base-content/45" title={series.location}>{series.location}</p>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className="badge badge-ghost gap-1"><BookOpen className="h-3 w-3" />{series.source === 'suwayomi' ? t('ext.detail.chapters', { count: Number(series.volume_count) }) : t('grid.volumes', { count: Number(series.volume_count) })}</span>
            <span className="badge badge-ghost">{t('detail.pages', { count: Number(series.page_count) })}</span>
            {series.has_text ? <span className={`badge gap-1 border ${MANGA.bgColor} ${MANGA.borderColor} ${MANGA.color}`}><ScanText className="h-3 w-3" />{t('detail.hasText')}</span> : null}
            {percent > 0 ? <span className="badge badge-ghost">{t('detail.percentRead', { percent })}</span> : null}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {next ? (
              <Button variant="primary" onClick={() => void openVolume(next.volume.id)}>
                <Play className="h-4 w-4" />
                {next.resume
                  ? t('actions.continueAt', { volume: next.volume.title, page: Number(next.volume.page ?? 0) + 1 })
                  : t('actions.readVolume', { volume: next.volume.title })}
              </Button>
            ) : null}
            {series.media_id ? (
              <span className="inline-flex min-w-0 items-center gap-1 rounded-field border border-base-300 bg-base-100/70 py-1 pl-3 pr-1 text-sm backdrop-blur-sm">
                <Link2 className="h-3.5 w-3.5 shrink-0 text-success" aria-hidden="true" />
                <span className="truncate" title={series.media_title ?? series.media_id}>{series.media_title ?? series.media_id}</span>
                <Button appearance="ghost" size="xs" shape="square" loading={unlinking} aria-label={t('detail.unlink')} title={t('detail.unlink')} onClick={() => void unlink()}>
                  {unlinking ? null : <Unlink className="h-3.5 w-3.5" />}
                </Button>
              </span>
            ) : (
              <Button appearance="outline" onClick={onLink}>
                <Link2 className="h-4 w-4" />
                {t('detail.link')}
              </Button>
            )}
          </div>
          {!series.media_id ? <p className="text-xs text-base-content/55">{t('detail.linkHint')}</p> : null}
        </div>
      </div>
    </section>
  );
}

/** Download state of a remote volume: a button, the bar while it runs, or "downloaded". */
function DownloadControl({ volume, progress, offline }: { volume: MangaVolume; progress?: DownloadProgress; offline: Unreachable }) {
  const { t, i18n } = useTranslation('manga');
  const [removing, setRemoving] = useState(false);
  if (progress) {
    const percent = progress.total ? Math.min(100, Math.round((Number(progress.received) / Number(progress.total)) * 100)) : null;
    return (
      <div className="flex w-44 shrink-0 items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-2 text-[11px] tabular-nums text-base-content/60">
            <span>{progress.offline ? t('webdav.download.downloading') : t('webdav.download.preparing')}</span>
            <span>{percent !== null ? `${percent}%` : formatBytes(Number(progress.received), i18n.language)}</span>
          </div>
          <progress
            className="progress progress-primary mt-1 h-1.5 w-full"
            value={percent ?? undefined}
            max={100}
            aria-label={t('webdav.download.progress', { title: volume.title })}
          />
        </div>
        <Button appearance="ghost" size="xs" shape="square" aria-label={t('webdav.download.cancel')} title={t('webdav.download.cancel')} onClick={() => void ipc.mangaCancelDownload(volume.id)}>
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    );
  }
  if (volume.downloaded) {
    const remove = async () => {
      setRemoving(true);
      try {
        await ipc.mangaRemoveDownload(volume.id);
      } finally {
        setRemoving(false);
      }
    };
    return (
      <div className="flex shrink-0 items-center gap-1">
        <span className="badge badge-soft badge-success badge-sm gap-1" title={t('webdav.download.downloadedHint')}>
          <HardDriveDownload className="h-3 w-3" aria-hidden="true" />
          {t('webdav.download.downloaded')}
        </span>
        <Button appearance="ghost" size="xs" shape="square" loading={removing} aria-label={t('webdav.download.remove', { title: volume.title })} title={t('webdav.download.remove', { title: volume.title })} onClick={() => void remove()}>
          {removing ? null : <Trash2 className="h-3.5 w-3.5" />}
        </Button>
      </div>
    );
  }
  return (
    <Button
      appearance="ghost"
      size="sm"
      shape="square"
      disabled={offline !== null}
      aria-label={t('webdav.download.action', { title: volume.title })}
      title={offline ? t(`webdav.download.${offline}`) : t('webdav.download.action', { title: volume.title })}
      onClick={() => void downloadForOffline(volume.id, volume.title)}
    >
      <CloudDownload className="h-4 w-4" />
    </Button>
  );
}

function VolumeRow({ volume, offline, ocr, source }: { volume: MangaVolume; offline: Unreachable; ocr: OcrStatus | undefined; source: MangaSource | null }) {
  const { t, i18n } = useTranslation('manga');
  const [exporting, setExporting] = useState(false);
  const progress = useDownloadStore((s) => s.running[volume.id]);
  const percent = readPercent(volume.page, Number(volume.page_count), volume.finished);
  const remote = isRemote(volume);
  const Format = isChapter(volume) ? Puzzle : volume.format === 'archive' || volume.format === 'remote-archive' ? FileArchive : Folder;
  // Offline, only what is on this computer can be opened.
  const unreachable = remote && offline !== null && !volume.downloaded;
  const disabled = !isReadable(volume) || unreachable || Boolean(progress && !progress.offline);
  return (
    // A row of the list's subgrid: the actions of every row share one column,
    // so the play buttons line up whatever each row offers.
    <li className={unreachable ? 'col-span-2 grid grid-cols-subgrid items-center opacity-60' : 'col-span-2 grid grid-cols-subgrid items-center'}>
      <button
        type="button"
        disabled={disabled}
        title={unreachable && offline ? t(`webdav.download.${offline}`) : undefined}
        onClick={() => void openVolume(volume.id)}
        className="group flex min-w-0 flex-1 items-center gap-4 py-3 pl-4 text-left motion-safe:transition-colors enabled:hover:bg-base-200/70 focus-visible:-outline-offset-2 disabled:cursor-not-allowed"
      >
        {isChapter(volume) ? (
          // A chapter has no image of its own until its pages are fetched.
          <span className={`grid aspect-[2/3] w-12 shrink-0 place-items-center rounded-field font-mono text-xs font-semibold tabular-nums ring-1 ring-base-content/5 ${MANGA.bgColor} ${MANGA.color}`}>
            {Number(volume.position)}
          </span>
        ) : (
          <div className="aspect-[2/3] w-12 shrink-0 overflow-hidden rounded-field shadow-sm ring-1 ring-base-content/5">
            <Cover src={mangaUrl.thumb(volume.id, 0)} />
          </div>
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate font-medium">{volume.title}</p>
          <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-xs text-base-content/55">
            <Format className="h-3 w-3" aria-hidden="true" />
            {volume.page_count > 0 ? t('detail.pages', { count: Number(volume.page_count) }) : isChapter(volume) ? t('ext.detail.pagesOnOpen') : remote ? t('webdav.detail.pagesOnOpen') : t('detail.pages', { count: 0 })}
            {remote && volume.size ? <><span aria-hidden="true">·</span>{formatBytes(Number(volume.size), i18n.language)}</> : null}
            <span aria-hidden="true">·</span>
            <OcrVolumeLabel volume={volume} />
          </p>
          {percent > 0 && !volume.finished ? (
            <div className="mt-2 flex items-center gap-2">
              <div className="h-1 max-w-56 flex-1 overflow-hidden rounded-full bg-base-300">
                <div className={`h-full rounded-full ${MANGA.accentColor}`} style={{ width: `${percent}%` }} />
              </div>
              <span className="text-[11px] tabular-nums text-base-content/55">{t('shelf.page', { page: Number(volume.page ?? 0) + 1, count: Number(volume.page_count) })}</span>
            </div>
          ) : null}
        </div>
        {volume.finished ? (
          <span className="badge badge-success badge-soft badge-sm shrink-0 gap-1"><Check className="h-3 w-3" />{t('detail.finished')}</span>
        ) : null}
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-full text-base-content/40 motion-safe:transition group-enabled:group-hover:bg-base-100 group-enabled:group-hover:text-base-content group-enabled:group-hover:shadow-sm">
          {unreachable ? <CloudOff className="h-4 w-4" aria-hidden="true" /> : <Play className="h-4 w-4" aria-hidden="true" />}
        </span>
      </button>
      <div className="flex items-center justify-end gap-2 pr-3 empty:pr-0">
      {volume.text_layers.length > 0 ? (
        <DropdownSelect
          className="select select-sm w-36 shrink-0"
          aria-label={t('webdav.detail.layer', { title: volume.title })}
          title={t('webdav.detail.layerHint')}
          value={volume.text_layer ?? ''}
          onChange={(event) => void ipc.mangaSetTextLayer(volume.id, event.target.value || null)}
        >
          <option value="">{t('webdav.detail.layerMain')}</option>
          {volume.text_layers.map((layer) => <option key={layer} value={layer}>{layer}</option>)}
        </DropdownSelect>
      ) : null}
      <OcrVolumeState volume={volume} status={ocr} disabled={unreachable || !isReadable(volume)} />
      {Number(volume.ocr_pages) > 0 && !isChapter(volume) ? (
        <>
          <Button
            appearance="ghost"
            size="sm"
            shape="square"
            disabled={remote && offline !== null}
            aria-label={t(remote ? 'ocr.export.titleRemote' : 'ocr.export.title')}
            title={t(remote ? 'ocr.export.titleRemote' : 'ocr.export.title')}
            onClick={() => setExporting(true)}
          >
            {remote ? <UploadCloud className="h-4 w-4" /> : <FileDown className="h-4 w-4" />}
          </Button>
          <OcrExportModal open={exporting} onClose={() => setExporting(false)} volume={volume} source={source} />
        </>
      ) : null}
      {remote ? <DownloadControl volume={volume} progress={progress} offline={offline} /> : null}
      </div>
    </li>
  );
}

/** Asks the extension's source again for the series' chapters. */
function RefreshChapters({ seriesId }: { seriesId: string }) {
  const { t } = useTranslation('manga');
  const [refreshing, setRefreshing] = useState(false);
  const refresh = async () => {
    setRefreshing(true);
    try {
      await ipc.mangaExtRefreshSeries(seriesId);
      toast.success(t('ext.detail.refreshed'));
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setRefreshing(false);
    }
  };
  return (
    <Button appearance="ghost" size="xs" shape="square" loading={refreshing} aria-label={t('ext.manga.refresh')} title={t('ext.manga.refresh')} onClick={() => void refresh()}>
      {refreshing ? null : <RefreshCw className="h-3.5 w-3.5" />}
    </Button>
  );
}

/** The server is not answering, or refuses the credentials: what can still
 *  be read, and a way to retry. */
function OfflineBanner({ source, reason }: { source: MangaSource; reason: 'offline' | 'unauthorized' }) {
  const { t } = useTranslation('manga');
  const refused = reason === 'unauthorized';
  return (
    <div role="status" className={refused ? 'surface-muted flex flex-wrap items-center gap-3 border border-error/30 px-4 py-3' : 'surface-muted flex flex-wrap items-center gap-3 border border-warning/30 px-4 py-3'}>
      <span className={refused ? 'grid h-9 w-9 shrink-0 place-items-center rounded-full bg-error/15 text-error' : 'grid h-9 w-9 shrink-0 place-items-center rounded-full bg-warning/15 text-warning'}>
        {refused ? <ShieldAlert className="h-4 w-4" aria-hidden="true" /> : <CloudOff className="h-4 w-4" aria-hidden="true" />}
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold">{t(refused ? 'webdav.detail.unauthorizedTitle' : 'webdav.detail.offlineTitle', { name: source.name })}</p>
        <p className="text-xs text-base-content/60">{t(refused ? 'webdav.detail.unauthorizedBody' : 'webdav.detail.offlineBody')}</p>
      </div>
      <Button appearance="outline" size="sm" loading={source.syncing} onClick={() => void syncSource(source.id)}>
        {source.syncing ? null : <RefreshCw className="h-4 w-4" />}
        {t('webdav.detail.retry')}
      </Button>
    </div>
  );
}

function DetailSkeleton() {
  return (
    <div className="space-y-6" aria-busy="true">
      <div className="surface flex gap-6 p-6">
        <Skeleton className="aspect-[2/3] w-44 shrink-0 rounded-box bg-base-200" />
        <div className="flex-1 space-y-3 self-end">
          <Skeleton className="h-3 w-16 bg-base-200" />
          <Skeleton className="h-8 w-2/3 bg-base-200" />
          <Skeleton className="h-4 w-1/3 bg-base-200" />
          <Skeleton className="h-10 w-48 rounded-field bg-base-200" />
        </div>
      </div>
      <div className="surface space-y-px overflow-hidden">
        {Array.from({ length: 4 }, (_, index) => <Skeleton key={index} className="h-20 w-full rounded-none bg-base-200" />)}
      </div>
    </div>
  );
}

export default function SeriesDetailScreen() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  const { seriesId = '' } = useParams();
  useMangaLibraryEvents();
  useDownloadEvents();
  const detail = useMangaSeries(seriesId);
  const ocr = useOcrStatus().data;
  const [linking, setLinking] = useState(false);
  const series = detail.data?.series;
  const suggestion = useSuggestions().byId.get(seriesId);
  const volumes = detail.data?.volumes ?? [];
  const source = detail.data?.source ?? null;
  const offline = unreachableReason(source);
  const remoteSeries = series?.source === 'webdav';
  const navigate = useNavigate();
  // Arrived by a view transition, the cover already moved: no fade on top.
  const [fade] = useState(() => !inViewTransition());

  // A remote series: its texts come down in the background while it is looked at.
  useEffect(() => {
    if (remoteSeries) void ipc.mangaPrepareSeries(seriesId).catch(() => undefined);
  }, [remoteSeries, seriesId]);

  return (
    <div className="min-h-full bg-base-300">
      <header className="sticky top-0 z-20 border-b border-base-300/60 bg-base-100">
        <div className="flex min-h-14 items-center gap-3 px-6 py-2.5">
          <Link
            to="/manga"
            className="btn btn-ghost btn-sm btn-square"
            aria-label={t('detail.back')}
            title={t('detail.back')}
            onClick={(event) => {
              if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
              event.preventDefault();
              backToLibrary(seriesId, navigate);
            }}
          >
            <ArrowLeft className="h-4 w-4" />
          </Link>
          <h1 className="min-w-0 flex-1 truncate text-base font-semibold leading-tight">{series?.title ?? t('title')}</h1>
        </div>
      </header>
      <div className={fade ? 'manga-fade-in space-y-6 px-6 py-5' : 'space-y-6 px-6 py-5'}>
        {detail.isPending ? (
          <DetailSkeleton />
        ) : detail.isError ? (
          <div className="surface"><ErrorState title={t('detail.loadFailed')} error={detail.error} retrying={detail.isFetching} onRetry={() => void detail.refetch()} /></div>
        ) : !series ? (
          <div className="surface">
            <EmptyState icon={BookOpen} title={t('detail.gone')} description={t('detail.goneBody')} action={<Link to="/manga" className="btn btn-outline btn-sm">{t('detail.back')}</Link>} />
          </div>
        ) : (
          <>
            {source && offline ? <OfflineBanner source={source} reason={offline} /> : null}
            <Hero series={series} volumes={volumes} source={source} onLink={() => setLinking(true)} />
            {suggestion && !series.media_id ? <SuggestionCard suggestion={suggestion} onChoose={() => setLinking(true)} /> : null}
            <section className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <h2 className="text-[11px] uppercase tracking-[0.2em] text-base-content/60">{series.source === 'suwayomi' ? t('ext.manga.chapters') : t('detail.volumes')}</h2>
                <span className="flex items-center gap-3 text-xs text-base-content/50">
                  {formatCount(volumes.length, i18n.language)}
                  {series.source === 'suwayomi' ? <RefreshChapters seriesId={series.id} /> : null}
                </span>
              </div>
              <ul className="surface grid grid-cols-[minmax(0,1fr)_auto] divide-y divide-base-300/60 overflow-hidden">
                {volumes.map((volume) => <VolumeRow key={volume.id} volume={volume} offline={offline} ocr={ocr} source={source} />)}
              </ul>
            </section>
            <OcrInstallModal />
            <MediaSearchModal
              open={linking}
              onClose={() => setLinking(false)}
              mediaType="manga"
              allowTypeChange={false}
              initialQuery={suggestion?.query ?? series.title}
              onPick={async (hit) => {
                try {
                  await ipc.mangaLinkSeries(series.id, hit.contentId, hit.title.contentTitleNative || hit.title.contentTitleRomaji || series.title, Boolean(hit.isAdult));
                  setLinking(false);
                  toast.success(t('toast.linked', { title: series.title }));
                } catch (error) {
                  toast.error(t('toast.linkFailed', { message: String(error) }));
                }
              }}
            />
          </>
        )}
      </div>
    </div>
  );
}
