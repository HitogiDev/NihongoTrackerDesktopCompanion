import { useEffect, useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Play, ScanText } from 'lucide-react';
import { mangaUrl, readPercent } from '../../lib/manga';
import type { MangaContinue, MangaSeries } from '../../lib/types';
import { Cover, CoverProgress, MANGA } from './Cover';
import { openVolume } from './useMangaLibrary';
import { openSeries } from './coverTransition';

const GRID = 'grid grid-cols-[repeat(auto-fill,minmax(8.5rem,1fr))] gap-x-4 gap-y-6';

/** "OCR" on a cover: this series has text to hover and look up. */
export function TextBadge({ className = '' }: { className?: string }) {
  const { t } = useTranslation('manga');
  return (
    <span title={t('grid.textHint')} className={`badge badge-sm gap-1 border-0 bg-neutral/80 font-semibold text-neutral-content shadow-sm backdrop-blur-sm ${className}`}>
      <ScanText className="h-3 w-3" aria-hidden="true" />
      {t('grid.text')}
    </span>
  );
}

export function SeriesCard({ series }: { series: MangaSeries }) {
  const { t } = useTranslation('manga');
  const navigate = useNavigate();
  const percent = series.page_count > 0 ? Math.round((series.pages_read / series.page_count) * 100) : 0;
  return (
    <li>
      <Link
        to={`/manga/${series.id}`}
        className="group block rounded-box focus-visible:outline-offset-4"
        title={series.title}
        onClick={(event) => openSeries(event, series.id, navigate)}
      >
        <div data-series-cover={series.id} className="relative aspect-[2/3] overflow-hidden rounded-box shadow-sm ring-1 ring-base-content/5 motion-safe:transition motion-safe:duration-200 motion-safe:ease-out group-hover:shadow-lg motion-safe:group-hover:-translate-y-1">
          <Cover src={mangaUrl.cover(series.id)} />
          <div className="pointer-events-none absolute inset-0 bg-linear-to-t from-neutral/40 via-transparent to-transparent opacity-0 motion-safe:transition-opacity group-hover:opacity-100" />
          {series.has_text ? <TextBadge className="absolute right-2 top-2" /> : null}
          <CoverProgress percent={percent} label={t('grid.progress', { percent })} />
        </div>
        <h3 className="mt-2.5 line-clamp-2 text-sm font-medium leading-snug group-hover:text-base-content">{series.title}</h3>
        <p className="mt-0.5 text-xs text-base-content/55">
          {t('grid.volumes', { count: Number(series.volume_count) })}
          {series.volumes_finished > 0 ? <> · {t('grid.finished', { count: Number(series.volumes_finished) })}</> : null}
        </p>
      </Link>
    </li>
  );
}

/** Cards mounted at first, and added each time the end of the grid shows. */
export const GRID_BATCH = 120;

/**
 * The library's series. A library of hundreds mounts them `GRID_BATCH` at a
 * time: the next batch comes in when the end of the grid scrolls into view
 * (or Tab reaches it), so opening the screen never builds every card.
 */
export function SeriesGrid({ series }: { series: MangaSeries[] }) {
  // Without IntersectionObserver nothing would ever add a batch: all of them.
  const [shown, setShown] = useState(() => (typeof IntersectionObserver === 'undefined' ? Infinity : GRID_BATCH));
  const end = useRef<HTMLDivElement>(null);
  const more = series.length > shown;
  useEffect(() => {
    const sentinel = end.current;
    if (!more || !sentinel) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) setShown((count) => count + GRID_BATCH);
    }, { rootMargin: '600px 0px' });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [more, shown]);
  return (
    <>
      <ul className={GRID}>{series.slice(0, shown).map((item) => <SeriesCard key={item.id} series={item} />)}</ul>
      {more ? (
        <div ref={end} className="flex justify-center py-4" aria-hidden="true">
          <span className="loading loading-dots loading-sm text-base-content/40" />
        </div>
      ) : null}
    </>
  );
}

export function SeriesGridSkeleton({ count = 10 }: { count?: number }) {
  const { t } = useTranslation('common');
  return (
    <ul className={GRID} aria-busy="true" aria-label={t('loading')}>
      {Array.from({ length: count }, (_, index) => (
        <li key={index}>
          <div className="skeleton aspect-[2/3] w-full rounded-box bg-base-100" />
          <div className="skeleton mt-3 h-3.5 w-4/5 bg-base-100" />
          <div className="skeleton mt-2 h-3 w-1/3 bg-base-100" />
        </li>
      ))}
    </ul>
  );
}

/** The "continue reading" shelf: bigger covers that open the reader directly. */
export function ContinueShelf({ items }: { items: MangaContinue[] }) {
  const { t } = useTranslation('manga');
  return (
    <ul className="-mx-6 flex snap-x snap-mandatory gap-4 overflow-x-auto px-6 pb-3 pt-1">
      {items.map((item) => {
        const percent = readPercent(item.page, Number(item.page_count));
        return (
          <li key={item.volume_id} className="w-48 shrink-0 snap-start xl:w-52">
            <button
              type="button"
              className="group block w-full rounded-box text-left focus-visible:outline-offset-4"
              onClick={() => void openVolume(item.volume_id)}
              title={t('shelf.open', { title: item.series_title, volume: item.volume_title })}
            >
              <div className="relative aspect-[2/3] overflow-hidden rounded-box shadow-sm ring-1 ring-base-content/5 motion-safe:transition motion-safe:duration-200 motion-safe:ease-out group-hover:shadow-lg motion-safe:group-hover:-translate-y-1">
                <Cover src={mangaUrl.thumb(item.volume_id, 0)} eager />
                <div className="absolute inset-0 grid place-items-center bg-neutral/0 motion-safe:transition-colors group-hover:bg-neutral/30">
                  <span className="grid h-12 w-12 scale-90 place-items-center rounded-full bg-base-100/90 text-base-content opacity-0 shadow-lg motion-safe:transition group-hover:scale-100 group-hover:opacity-100">
                    <Play className="ml-0.5 h-5 w-5" aria-hidden="true" />
                  </span>
                </div>
                {item.up_next ? (
                  <span className={`badge badge-sm absolute left-2 top-2 border-0 bg-base-100/90 font-semibold shadow-sm backdrop-blur-sm ${MANGA.color}`}>{t('shelf.upNext')}</span>
                ) : null}
                <CoverProgress percent={item.up_next ? 0 : percent} label={t('grid.progress', { percent })} />
              </div>
              <h3 className="mt-2.5 truncate text-sm font-semibold">{item.series_title}</h3>
              <p className="truncate text-xs text-base-content/60">
                {item.volume_title}
                {!item.up_next ? <> · {t('shelf.page', { page: Number(item.page) + 1, count: Number(item.page_count) })}</> : null}
              </p>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

export function ShelfSkeleton() {
  return (
    <div className="flex gap-4 overflow-hidden pb-3 pt-1" aria-hidden="true">
      {Array.from({ length: 5 }, (_, index) => (
        <div key={index} className="w-48 shrink-0 xl:w-52">
          <div className="skeleton aspect-[2/3] w-full rounded-box bg-base-100" />
          <div className="skeleton mt-3 h-3.5 w-3/4 bg-base-100" />
        </div>
      ))}
    </div>
  );
}
