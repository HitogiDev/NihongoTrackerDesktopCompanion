import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, BookOpen, Check, FileArchive, Folder, Link2, Play, ScanText, Unlink } from 'lucide-react';
import { MediaSearchModal } from '../../components/MediaSearchModal';
import { Button } from '../../components/ui/Button';
import { EmptyState, ErrorState, Skeleton } from '../../components/ui/Empty';
import { ipc } from '../../lib/ipc';
import { mangaUrl, readPercent } from '../../lib/manga';
import type { MangaSeries, MangaVolume } from '../../lib/types';
import { toast } from '../../store/useToastStore';
import { Cover, MANGA } from './Cover';
import { openVolume, useMangaLibraryEvents, useMangaSeries } from './useMangaLibrary';

/** The volume "Read" opens: the one in progress, else the first unread one. */
export function nextVolume(volumes: MangaVolume[]): { volume: MangaVolume; resume: boolean } | null {
  const readable = volumes.filter((volume) => volume.page_count > 0);
  const started = readable
    .filter((volume) => volume.page !== null && !volume.finished)
    .sort((a, b) => (b.read_at ?? '').localeCompare(a.read_at ?? ''))[0];
  if (started) return { volume: started, resume: true };
  const unread = readable.find((volume) => !volume.finished);
  if (unread) return { volume: unread, resume: false };
  return readable[0] ? { volume: readable[0], resume: false } : null;
}

function Hero({ series, volumes, onLink }: { series: MangaSeries; volumes: MangaVolume[]; onLink: () => void }) {
  const { t } = useTranslation(['manga', 'common']);
  const next = nextVolume(volumes);
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
        <div className="aspect-[2/3] w-36 shrink-0 overflow-hidden rounded-box shadow-lg ring-1 ring-base-content/10 sm:w-44">
          <Cover src={cover} eager />
        </div>
        <div className="min-w-0 flex-1 space-y-4">
          <div>
            <p className={`text-xs font-semibold uppercase tracking-[0.18em] ${MANGA.color}`}>{t('common:logType.manga')}</p>
            <h2 className="mt-1 text-2xl font-bold leading-tight sm:text-3xl">{series.title}</h2>
            <p className="selectable mt-1 truncate font-mono text-[11px] text-base-content/45" title={series.location}>{series.location}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <span className="badge badge-ghost gap-1"><BookOpen className="h-3 w-3" />{t('grid.volumes', { count: Number(series.volume_count) })}</span>
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

function VolumeRow({ volume }: { volume: MangaVolume }) {
  const { t } = useTranslation('manga');
  const percent = readPercent(volume.page, Number(volume.page_count), volume.finished);
  const Format = volume.format === 'archive' ? FileArchive : Folder;
  return (
    <li>
      <button
        type="button"
        disabled={volume.page_count === 0}
        onClick={() => void openVolume(volume.id)}
        className="group flex w-full items-center gap-4 px-4 py-3 text-left motion-safe:transition-colors hover:bg-base-200/70 focus-visible:-outline-offset-2"
      >
        <div className="aspect-[2/3] w-12 shrink-0 overflow-hidden rounded-field shadow-sm ring-1 ring-base-content/5">
          <Cover src={mangaUrl.thumb(volume.id, 0)} />
        </div>
        <div className="min-w-0 flex-1">
          <p className="truncate font-medium">{volume.title}</p>
          <p className="mt-0.5 flex items-center gap-1.5 text-xs text-base-content/55">
            <Format className="h-3 w-3" aria-hidden="true" />
            {t('detail.pages', { count: Number(volume.page_count) })}
            {volume.ocr_state !== 'none'
              ? <><span aria-hidden="true">·</span><span className={MANGA.color}>{t(volume.ocr_state === 'mokuro' ? 'detail.textMokuro' : 'detail.textOcr')}</span></>
              : <><span aria-hidden="true">·</span>{t('detail.noText')}</>}
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
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-full text-base-content/40 motion-safe:transition group-hover:bg-base-100 group-hover:text-base-content group-hover:shadow-sm">
          <Play className="h-4 w-4" aria-hidden="true" />
        </span>
      </button>
    </li>
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
  const { t } = useTranslation(['manga', 'common']);
  const { seriesId = '' } = useParams();
  useMangaLibraryEvents();
  const detail = useMangaSeries(seriesId);
  const [linking, setLinking] = useState(false);
  const series = detail.data?.series;
  const volumes = detail.data?.volumes ?? [];

  return (
    <div className="min-h-full bg-base-300">
      <header className="sticky top-0 z-20 border-b border-base-300/60 bg-base-100">
        <div className="flex min-h-14 items-center gap-3 px-6 py-2.5">
          <Link to="/manga" className="btn btn-ghost btn-sm btn-square" aria-label={t('detail.back')} title={t('detail.back')}>
            <ArrowLeft className="h-4 w-4" />
          </Link>
          <h1 className="min-w-0 flex-1 truncate text-base font-semibold leading-tight">{series?.title ?? t('title')}</h1>
        </div>
      </header>
      <div className="space-y-6 px-6 py-5">
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
            <Hero series={series} volumes={volumes} onLink={() => setLinking(true)} />
            <section className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <h2 className="text-[11px] uppercase tracking-[0.2em] text-base-content/60">{t('detail.volumes')}</h2>
                <span className="text-xs text-base-content/50">{volumes.length}</span>
              </div>
              <ul className="surface divide-y divide-base-300/60 overflow-hidden">
                {volumes.map((volume) => <VolumeRow key={volume.id} volume={volume} />)}
              </ul>
            </section>
            <MediaSearchModal
              open={linking}
              onClose={() => setLinking(false)}
              mediaType="manga"
              allowTypeChange={false}
              initialQuery={series.title}
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
