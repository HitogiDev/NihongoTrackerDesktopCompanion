import { useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ArrowLeft, Check, Compass, Flame, Search, SearchX, Sparkles } from 'lucide-react';
import { EmptyState, ErrorState } from '../../../components/ui/Empty';
import { ipc } from '../../../lib/ipc';
import { mangaUrl } from '../../../lib/manga';
import type { ExtBrowseKind, ExtManga } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { Cover, MANGA } from '../Cover';
import { SeriesGridSkeleton } from '../SeriesGrid';
import { ContentBadge, ExtIcon } from './ExtStore';
import { EXT_SOURCES_QUERY, extErrorText } from './useExtensions';

const GRID = 'grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-4 gap-y-6';

/** A series of a catalogue: the same card as the library's. */
export function ExtMangaCard({ manga }: { manga: ExtManga }) {
  const { t } = useTranslation('manga');
  return (
    <li>
      <Link to={`/manga/ext/manga/${manga.id}`} className="group block rounded-box focus-visible:outline-offset-4" title={manga.title}>
        <div className="relative aspect-[2/3] overflow-hidden rounded-box shadow-sm ring-1 ring-base-content/5 motion-safe:transition motion-safe:duration-200 motion-safe:ease-out group-hover:shadow-lg motion-safe:group-hover:-translate-y-1">
          {manga.thumb ? <Cover src={mangaUrl.ext(manga.thumb)} /> : <div className="h-full w-full bg-base-100" />}
          <div className="pointer-events-none absolute inset-0 bg-linear-to-t from-neutral/40 via-transparent to-transparent opacity-0 motion-safe:transition-opacity group-hover:opacity-100" />
          {manga.series_id ? (
            <span className={`badge badge-sm absolute left-2 top-2 gap-1 border-0 bg-base-100/90 font-semibold shadow-sm backdrop-blur-sm ${MANGA.color}`}>
              <Check className="h-3 w-3" aria-hidden="true" />
              {t('ext.browse.inLibrary')}
            </span>
          ) : null}
        </div>
        <h3 className="mt-2.5 line-clamp-2 text-sm font-medium leading-snug group-hover:text-base-content">{manga.title}</h3>
      </Link>
    </li>
  );
}

/** One source's catalogue: popular, latest or a search, loaded page by page as it scrolls. */
export default function ExtBrowseScreen() {
  const { t } = useTranslation(['manga', 'common']);
  const { sourceId = '' } = useParams();
  const showAdult = useAppStore((s) => s.settings?.manga_ext_show_adult ?? false);
  const sources = useQuery({ queryKey: [...EXT_SOURCES_QUERY, showAdult], queryFn: ipc.mangaExtSources });
  const source = sources.data?.find((item) => item.id === sourceId);
  const [kind, setKind] = useState<ExtBrowseKind>('popular');
  const [draft, setDraft] = useState('');
  const [query, setQuery] = useState('');
  const sentinel = useRef<HTMLDivElement>(null);

  const results = useInfiniteQuery({
    queryKey: ['manga', 'ext', 'browse', sourceId, kind, query],
    queryFn: ({ pageParam }) => ipc.mangaExtBrowse(sourceId, kind, pageParam, kind === 'search' ? query : undefined),
    initialPageParam: 1,
    getNextPageParam: (last, pages) => (last.has_next ? pages.length + 1 : undefined),
    staleTime: 10 * 60_000,
    enabled: kind !== 'search' || query !== '',
  });

  // The next page loads as the end of the grid comes into view.
  useEffect(() => {
    const node = sentinel.current;
    if (!node) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting) && results.hasNextPage && !results.isFetchingNextPage) void results.fetchNextPage();
    }, { rootMargin: '600px' });
    observer.observe(node);
    return () => observer.disconnect();
  }, [results.hasNextPage, results.isFetchingNextPage, results.fetchNextPage]);

  const mangas = (results.data?.pages ?? []).flatMap((page) => page.mangas);
  // A source repeats series across pages: show each once.
  const unique = mangas.filter((manga, index) => mangas.findIndex((other) => other.id === manga.id) === index);
  const tabs: { id: ExtBrowseKind; icon: typeof Flame; show: boolean }[] = [
    { id: 'popular', icon: Flame, show: true },
    { id: 'latest', icon: Sparkles, show: source?.supports_latest ?? true },
  ];

  return (
    <div className="min-h-full bg-base-300">
      <header className="sticky top-0 z-20 border-b border-base-300/60 bg-base-100">
        <div className="flex min-h-14 flex-wrap items-center gap-3 px-6 py-2.5">
          <Link to="/manga" state={{ origin: 'extensions' }} className="btn btn-ghost btn-sm btn-square" aria-label={t('detail.back')} title={t('detail.back')}>
            <ArrowLeft className="h-4 w-4" />
          </Link>
          <ExtIcon token={source?.icon ?? null} className="h-8 w-8" />
          <div className="min-w-0 flex-1">
            <h1 className="flex items-center gap-2 truncate text-base font-semibold leading-tight">
              {source?.name ?? t('ext.browse.title')}
              {source ? <ContentBadge content={source.content} /> : null}
            </h1>
          </div>
          <div role="tablist" aria-label={t('ext.browse.kind')} className="tabs tabs-box tabs-sm">
            {tabs.filter((tab) => tab.show).map(({ id, icon: Icon }) => (
              <button key={id} type="button" role="tab" aria-selected={kind === id} className={kind === id ? 'tab tab-active gap-1.5' : 'tab gap-1.5'} onClick={() => setKind(id)}>
                <Icon className="h-3.5 w-3.5" aria-hidden="true" />
                {t(`ext.browse.${id}`)}
              </button>
            ))}
          </div>
          <form
            className="w-full sm:w-64"
            role="search"
            onSubmit={(event) => {
              event.preventDefault();
              const text = draft.trim();
              setQuery(text);
              setKind(text ? 'search' : 'popular');
            }}
          >
            <label className="input input-sm w-full">
              <Search className="h-4 w-4 opacity-60" />
              <input type="search" value={draft} onChange={(event) => setDraft(event.target.value)} placeholder={t('ext.browse.search')} aria-label={t('ext.browse.search')} />
            </label>
          </form>
        </div>
      </header>
      <div className="space-y-6 px-6 py-5">
        {kind === 'search' && query ? <p className="text-sm text-base-content/60">{t('ext.browse.resultsFor', { query })}</p> : null}
        {results.isPending && results.fetchStatus !== 'idle' ? (
          <SeriesGridSkeleton count={18} />
        ) : results.isError ? (
          <div className="surface"><ErrorState title={t('ext.browse.failed')} error={extErrorText(results.error)} retrying={results.isFetching} onRetry={() => void results.refetch()} /></div>
        ) : unique.length === 0 ? (
          <div className="surface"><EmptyState icon={kind === 'search' ? SearchX : Compass} title={t('ext.browse.none')} /></div>
        ) : (
          <ul className={GRID}>{unique.map((manga) => <ExtMangaCard key={manga.id} manga={manga} />)}</ul>
        )}
        <div ref={sentinel} aria-hidden="true" />
        {results.isFetchingNextPage ? <SeriesGridSkeleton count={6} /> : null}
        {!results.hasNextPage && unique.length > 0 ? <p className="pb-4 text-center text-xs text-base-content/45">{t('ext.browse.end')}</p> : null}
      </div>
    </div>
  );
}
