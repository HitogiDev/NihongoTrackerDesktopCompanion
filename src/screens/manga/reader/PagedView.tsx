import { useEffect, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit, ReadingDirection } from '../../../lib/types';
import { fitSpread, onScreen, type Size } from './layout';
import { PageImage } from './PageImage';

/**
 * One spread (one or two pages) at a time. A new spread fades in briefly
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
  const pages = onScreen(spread, direction);
  const sizes = fitSpread(pages.map(sizeOf), viewport, fit);
  const key = spread.join('-');

  useEffect(() => {
    scroller.current?.scrollTo({ top: 0, left: direction === 'rtl' ? scroller.current.scrollWidth : 0 });
  }, [key, direction]);

  return (
    <div ref={scroller} className={fit === 'height' ? 'h-full w-full overflow-hidden' : 'h-full w-full overflow-auto'}>
      <div key={key} className="manga-page-in flex min-h-full min-w-full w-max items-center justify-center">
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
