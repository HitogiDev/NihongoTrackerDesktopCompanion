import { forwardRef, useEffect, useImperativeHandle, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit } from '../../../lib/types';
import { fitSpread, type Size } from './layout';
import { PageImage } from './PageImage';

export type StripHandle = { scrollToPage: (page: number, smooth?: boolean) => void; scrollBy: (fraction: number) => void };

/**
 * Every page one under the other, for webtoons. The current page is the one
 * across the upper part of the window; scrolling reports it, and the slider
 * or Home/End scroll to a page.
 */
export const VerticalStrip = forwardRef<StripHandle, {
  volumeId: string;
  pageCount: number;
  initialPage: number;
  fit: PageFit;
  viewport: Size;
  sizeOf: (page: number) => Size;
  onNaturalSize: (page: number, size: Size) => void;
  onPage: (page: number) => void;
  overlay?: (page: number, size: Size) => ReactNode;
}>(function VerticalStrip({ volumeId, pageCount, initialPage, fit, viewport, sizeOf, onNaturalSize, onPage, overlay }, ref) {
  const { t } = useTranslation('mangaReader');
  const scroller = useRef<HTMLDivElement>(null);
  const reported = useRef(initialPage);

  useImperativeHandle(ref, () => ({
    scrollToPage(page, smooth = false) {
      const element = scroller.current?.querySelector<HTMLElement>(`[data-page="${page}"]`);
      const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      element?.scrollIntoView({ block: 'start', behavior: smooth && !reduce ? 'smooth' : 'auto' });
    },
    scrollBy(fraction) {
      scroller.current?.scrollBy({ top: fraction * (scroller.current.clientHeight || 0) });
    },
  }), []);

  // Start where the user left off, once the pages above have their size.
  useEffect(() => {
    const element = scroller.current?.querySelector<HTMLElement>(`[data-page="${initialPage}"]`);
    element?.scrollIntoView({ block: 'start' });
  }, []);

  const onScroll = () => {
    const box = scroller.current;
    if (!box) return;
    const line = box.getBoundingClientRect().top + box.clientHeight * 0.35;
    const pages = box.querySelectorAll<HTMLElement>('[data-page]');
    let current = 0;
    for (const element of pages) {
      if (element.getBoundingClientRect().top <= line) current = Number(element.dataset.page);
      else break;
    }
    if (current !== reported.current) {
      reported.current = current;
      onPage(current);
    }
  };

  return (
    <div ref={scroller} className="h-full w-full overflow-y-auto overflow-x-hidden" onScroll={onScroll}>
      <div className="flex flex-col items-center gap-1.5 py-1.5">
        {Array.from({ length: pageCount }, (_, page) => {
          const [size] = fitSpread([sizeOf(page)], viewport, fit);
          return (
            <PageImage
              key={page}
              page={page}
              src={mangaUrl.page(volumeId, page)}
              alt={t('pageAlt', { page: page + 1 })}
              size={size}
              eager={Math.abs(page - initialPage) <= 2}
              failedLabel={t('pageFailed')}
              onNaturalSize={(natural) => onNaturalSize(page, natural)}
            >
              {overlay?.(page, size)}
            </PageImage>
          );
        })}
      </div>
    </div>
  );
});
