import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit, ReadingDirection } from '../../../lib/types';
import { fitSpread, type Size } from './layout';
import { PageImage } from './PageImage';

/** How long a page turn waits for the next spread to decode before showing
 *  it anyway, with its spinner. Preloaded spreads are ready at once. */
const DECODE_WAIT_MS = 250;

/**
 * One spread (one or two pages) at a time. Turning the page keeps the old
 * spread up until the new one is decoded (`decode()`, at most
 * `DECODE_WAIT_MS`), so a page never paints in halves. A new spread fades in briefly
 * (`manga-page-in`, off with reduced motion) and starts at the top when the
 * fit makes it taller than the window.
 */
export function PagedView({ volumeId, spread, direction, fit, viewport, sizeOf, onNaturalSize, overlay }: {
  volumeId: string;
  spread: number[];
  direction: ReadingDirection;
  fit: PageFit;
  viewport: Size;
  sizeOf: (page: number) => Size;
  onNaturalSize: (page: number, size: Size) => void;
  /** What to draw over a page (the text layer), given its drawn size. */
  overlay?: (page: number, size: Size) => ReactNode;
}) {
  const { t } = useTranslation('mangaReader');
  const scroller = useRef<HTMLDivElement>(null);
  const wanted = spread.join('-');
  const [pages, setPages] = useState(spread);
  const key = pages.join('-');
  useEffect(() => {
    if (wanted === key) return;
    let alive = true;
    const decoded = spread.map((page) => {
      const image = new Image();
      image.src = mangaUrl.page(volumeId, page);
      return image.decode().catch(() => undefined);
    });
    const waited = new Promise((resolve) => window.setTimeout(resolve, DECODE_WAIT_MS));
    void Promise.race([Promise.all(decoded), waited]).then(() => { if (alive) setPages(spread); });
    return () => { alive = false; };
    // `spread` is new on every render; `wanted` is what it says.
  }, [wanted, key, volumeId]);
  // In reading order, laid out right to left for an RTL book: Tab and screen
  // readers then meet the first page first.
  const sizes = fitSpread(pages.map(sizeOf), viewport, fit);

  useEffect(() => {
    scroller.current?.scrollTo({ top: 0, left: direction === 'rtl' ? scroller.current.scrollWidth : 0 });
  }, [key, direction]);

  return (
    <div ref={scroller} className={fit === 'height' ? 'h-full w-full overflow-hidden' : 'h-full w-full overflow-auto'}>
      <div key={key} className={direction === 'rtl' ? 'manga-page-in flex min-h-full min-w-full w-max flex-row-reverse items-center justify-center' : 'manga-page-in flex min-h-full min-w-full w-max items-center justify-center'}>
        {pages.map((page, index) => (
          <PageImage
            key={page}
            page={page}
            src={mangaUrl.page(volumeId, page)}
            alt={t('pageAlt', { page: page + 1 })}
            size={sizes[index]}
            failedLabel={t('pageFailed')}
            onNaturalSize={(size) => onNaturalSize(page, size)}
          >
            {overlay?.(page, sizes[index])}
          </PageImage>
        ))}
      </div>
    </div>
  );
}
