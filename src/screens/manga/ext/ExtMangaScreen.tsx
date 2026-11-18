import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, BookOpen, Check, LibraryBig, Play, Plus, RefreshCw, ShieldAlert } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { EmptyState, ErrorState, Skeleton } from '../../../components/ui/Empty';
import { ipc } from '../../../lib/ipc';
import { formatCount, mangaUrl } from '../../../lib/manga';
import type { ExtChapter, ExtMangaDetail } from '../../../lib/types';
import { toast } from '../../../store/useToastStore';
import { Cover, MANGA } from '../Cover';
import { useMangaLibraryEvents } from '../useMangaLibrary';
import { ContentBadge, ExtIcon } from './ExtStore';
import { extErrorText } from './useExtensions';

/** `2023-04-01` from milliseconds, in the user's language. */
function uploadDate(ms: number | null, locale: string): string | null {
  return ms ? new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(new Date(ms)) : null;
}

function ChapterRow({ chapter, opening, onRead }: { chapter: ExtChapter; opening: boolean; onRead: () => void }) {
  const { t, i18n } = useTranslation('manga');
  const date = uploadDate(chapter.uploaded_at === null ? null : Number(chapter.uploaded_at), i18n.language);
  return (
    <li>
      <button
        type="button"
        disabled={opening}
        onClick={onRead}
        className="group flex w-full min-w-0 items-center gap-4 px-4 py-3 text-left motion-safe:transition-colors enabled:hover:bg-base-200/70 focus-visible:-outline-offset-2"
      >
        <span className="w-10 shrink-0 text-right font-mono text-xs tabular-nums text-base-content/40">{Number(chapter.position)}</span>
        <div className="min-w-0 flex-1">
          <p className="truncate font-medium">{chapter.name}</p>
          <p className="mt-0.5 flex flex-wrap gap-x-1.5 text-xs text-base-content/55">
            {date ? <span>{date}</span> : null}
            {chapter.scanlator ? <><span aria-hidden="true">·</span><span className="truncate">{chapter.scanlator}</span></> : null}
            {Number(chapter.page_count) > 0 ? <><span aria-hidden="true">·</span><span>{t('detail.pages', { count: Number(chapter.page_count) })}</span></> : null}
          </p>
        </div>
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-full text-base-content/40 motion-safe:transition group-enabled:group-hover:bg-base-100 group-enabled:group-hover:text-base-content group-enabled:group-hover:shadow-sm">
          {opening ? <span className="loading loading-spinner loading-xs" aria-hidden="true" /> : <Play className="h-4 w-4" aria-hidden="true" />}
        </span>
      </button>
    </li>
  );
}

function Hero({ detail, onAdd, adding }: { detail: ExtMangaDetail; onAdd: () => void; adding: boolean }) {
  const { t } = useTranslation(['manga', 'common']);
  const cover = detail.thumb ? mangaUrl.ext(detail.thumb) : null;
  const [more, setMore] = useState(false);
  return (
    <section className="surface relative isolate overflow-hidden">
      {cover ? <img src={cover} alt="" aria-hidden="true" draggable={false} className="absolute inset-0 -z-10 h-full w-full scale-125 object-cover opacity-80 blur-3xl saturate-150" /> : null}
      <div className="absolute inset-0 -z-10 bg-linear-to-r from-base-100/95 via-base-100/75 to-base-100/30" />
      <div className="flex flex-col gap-6 p-6 sm:flex-row sm:items-end">
        <div className="aspect-[2/3] w-36 shrink-0 overflow-hidden rounded-box shadow-lg ring-1 ring-base-content/10 sm:w-44">
          {cover ? <Cover src={cover} eager /> : <div className="h-full w-full bg-base-200" />}
        </div>
        <div className="min-w-0 flex-1 space-y-4">
          <div>
            <p className={`text-xs font-semibold uppercase tracking-[0.18em] ${MANGA.color}`}>{t('common:logType.manga')}</p>
            <h2 className="mt-1 text-2xl font-bold leading-tight sm:text-3xl">{detail.title}</h2>
            {detail.author || detail.artist ? (
              <p className="mt-1 text-sm text-base-content/65">{[detail.author, detail.artist].filter((name, index, all) => name && all.indexOf(name) === index).join(' · ')}</p>
            ) : null}
            {detail.source ? (
              <p className="mt-2 flex items-center gap-2 text-xs text-base-content/60">
                <ExtIcon token={detail.source.icon} className="h-5 w-5" />
                <span>{detail.source.name}</span>
                <ContentBadge content={detail.source.content} />
              </p>
            ) : null}
          </div>
          {detail.genres.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">{detail.genres.slice(0, 10).map((genre) => <span key={genre} className="badge badge-ghost badge-sm">{genre}</span>)}</div>
          ) : null}
          {detail.description ? (
            <div>
              <p className={more ? 'selectable whitespace-pre-line text-sm text-base-content/75' : 'selectable line-clamp-3 whitespace-pre-line text-sm text-base-content/75'}>{detail.description}</p>
              {detail.description.length > 180 ? (
                <button type="button" className="link link-hover mt-1 text-xs text-base-content/60" onClick={() => setMore(!more)}>{more ? t('ext.manga.less') : t('ext.manga.more')}</button>
              ) : null}
            </div>
          ) : null}
          <div className="flex flex-wrap items-center gap-2">
            {detail.series_id ? (
              <Link to={`/manga/${detail.series_id}`} className="btn btn-outline">
                <LibraryBig className="h-4 w-4" />
                {t('ext.manga.inLibrary')}
              </Link>
            ) : (
              <Button variant="primary" loading={adding} onClick={onAdd}>
                {adding ? null : <Plus className="h-4 w-4" />}
                {t('ext.manga.add')}
              </Button>
            )}
          </div>
          {detail.adult ? (
            <p className="flex items-start gap-1.5 text-xs text-base-content/55">
              <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              {t('ext.manga.adult')}
            </p>
          ) : null}
        </div>
      </div>
    </section>
  );
}

/** A series of a source: what it is, its chapters, and "add to library". */
export default function ExtMangaScreen() {
  const { t, i18n } = useTranslation(['manga', 'common']);
  const { mangaId = '' } = useParams();
  const id = Number(mangaId);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  useMangaLibraryEvents();
  const queryKey = ['manga', 'ext', 'manga', id];
  const detail = useQuery({ queryKey, queryFn: () => ipc.mangaExtManga(id), enabled: Number.isFinite(id) });
  const [adding, setAdding] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [opening, setOpening] = useState<number | null>(null);
  const data = detail.data;
  const chapters = data?.chapters ?? [];

  const add = async () => {
    setAdding(true);
    try {
      const seriesId = await ipc.mangaExtAddSeries(id);
      queryClient.setQueryData<ExtMangaDetail>(queryKey, (current) => (current ? { ...current, series_id: seriesId } : current));
      toast.success(t('ext.manga.added', { title: data?.title ?? '' }));
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setAdding(false);
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      queryClient.setQueryData(queryKey, await ipc.mangaExtManga(id, true));
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setRefreshing(false);
    }
  };

  const read = async (chapter: ExtChapter) => {
    setOpening(Number(chapter.id));
    try {
      const seriesId = await ipc.mangaExtRead(id, Number(chapter.id));
      queryClient.setQueryData<ExtMangaDetail>(queryKey, (current) => (current ? { ...current, series_id: seriesId } : current));
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setOpening(null);
    }
  };

  return (
    <div className="min-h-full bg-base-300">
      <header className="sticky top-0 z-20 border-b border-base-300/60 bg-base-100">
        <div className="flex min-h-14 items-center gap-3 px-6 py-2.5">
          <button type="button" className="btn btn-ghost btn-sm btn-square" aria-label={t('detail.back')} title={t('detail.back')} onClick={() => navigate(-1)}>
            <ArrowLeft className="h-4 w-4" />
          </button>
          <h1 className="min-w-0 flex-1 truncate text-base font-semibold leading-tight">{data?.title ?? t('title')}</h1>
        </div>
      </header>
      <div className="space-y-6 px-6 py-5">
        {detail.isPending ? (
          <div className="space-y-6" aria-busy="true">
            <div className="surface flex gap-6 p-6">
              <Skeleton className="aspect-[2/3] w-44 shrink-0 rounded-box bg-base-200" />
              <div className="flex-1 space-y-3 self-end">
                <Skeleton className="h-3 w-16 bg-base-200" />
                <Skeleton className="h-8 w-2/3 bg-base-200" />
                <Skeleton className="h-10 w-48 rounded-field bg-base-200" />
              </div>
            </div>
            <div className="surface space-y-px overflow-hidden">{Array.from({ length: 5 }, (_, index) => <Skeleton key={index} className="h-14 w-full rounded-none bg-base-200" />)}</div>
          </div>
        ) : detail.isError || !data ? (
          <div className="surface"><ErrorState title={t('ext.manga.failed')} error={extErrorText(detail.error)} retrying={detail.isFetching} onRetry={() => void detail.refetch()} /></div>
        ) : (
          <>
            <Hero detail={data} onAdd={() => void add()} adding={adding} />
            <section className="space-y-3">
              <div className="flex items-center justify-between gap-3">
                <h2 className="text-[11px] uppercase tracking-[0.2em] text-base-content/60">{t('ext.manga.chapters')}</h2>
                <span className="flex items-center gap-2 text-xs text-base-content/50">
                  {formatCount(chapters.length, i18n.language)}
                  <Button appearance="ghost" size="xs" shape="square" loading={refreshing} aria-label={t('ext.manga.refresh')} title={t('ext.manga.refresh')} onClick={() => void refresh()}>
                    {refreshing ? null : <RefreshCw className="h-3.5 w-3.5" />}
                  </Button>
                </span>
              </div>
              {chapters.length === 0 ? (
                <div className="surface"><EmptyState icon={BookOpen} title={t('ext.manga.noChapters')} compact /></div>
              ) : (
                <ul className="surface divide-y divide-base-300/60 overflow-hidden">
                  {chapters.map((chapter) => <ChapterRow key={chapter.id} chapter={chapter} opening={opening === Number(chapter.id)} onRead={() => void read(chapter)} />)}
                </ul>
              )}
              {data.series_id ? (
                <p className="flex items-center gap-1.5 text-xs text-base-content/55"><Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />{t('ext.manga.tracked')}</p>
              ) : (
                <p className="text-xs text-base-content/55">{t('ext.manga.readAdds')}</p>
              )}
            </section>
          </>
        )}
      </div>
    </div>
  );
}
