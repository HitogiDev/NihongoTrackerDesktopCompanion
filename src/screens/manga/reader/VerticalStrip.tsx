import { forwardRef, useCallback, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit } from '../../../lib/types';
import { fitSpread, type Size } from './layout';
import { PageImage } from './PageImage';
import { anchorAt, anchoredScroll, pageAt, stripOffsets, stripWindow } from './stripWindow';

export type StripHandle = { scrollToPage: (page: number, smooth?: boolean) => void; scrollBy: (fraction: number) => void };

/** The current page is the one across this fraction of the window, from the top. */
const CURRENT_LINE = 0.35;

/**
 * Every page one under the other, for webtoons. Only the pages near the
 * screen are mounted (`stripWindow.ts`), each at its offset in a strip as
 * tall as the whole volume; the scroll is anchored by hand when pages above
 * change height. The current page is the one across the upper part of the
 * window; scrolling reports it, and the slider or Home/End scroll to a page.
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

  const sizes = useMemo(
    () => Array.from({ length: pageCount }, (_, page) => fitSpread([sizeOf(page)], viewport, fit)[0]),
    [pageCount, sizeOf, viewport, fit],
  );
  const { offsets, total } = useMemo(() => stripOffsets(sizes.map((size) => size.height)), [sizes]);
  const width = useMemo(() => sizes.reduce((widest, size) => Math.max(widest, size.width), 0), [sizes]);

  // Where the top of the screen is, as a page and a distance into it: kept
  // across size changes (see `anchoredScroll`). Starts at the saved page.
  const anchor = useRef({ page: initialPage, within: 0 });
  const [range, setRange] = useState(() => stripWindow(offsets, offsets[initialPage] ?? 0, viewport.height));

  const update = useCallback((scrollTop: number, height: number) => {
    const next = stripWindow(offsets, scrollTop, height);
    setRange((current) => (current.start === next.start && current.end === next.end ? current : next));
    const current = pageAt(offsets, scrollTop + height * CURRENT_LINE);
    if (current !== reported.current) {
      reported.current = current;
      onPage(current);
    }
  }, [offsets, onPage]);

  // Before paint, whenever the offsets change (first layout, a page's real
  // size, a resize): put the anchored point back at the top of the screen.
  useLayoutEffect(() => {
    const box = scroller.current;
    if (!box) return;
    const target = anchoredScroll(offsets, anchor.current);
    if (Math.abs(box.scrollTop - target) > 0.5) box.scrollTop = target;
    update(box.scrollTop, box.clientHeight || viewport.height);
  }, [offsets, update, viewport.height]);

  const onScroll = () => {
    const box = scroller.current;
    if (!box) return;
    anchor.current = anchorAt(offsets, box.scrollTop);
    update(box.scrollTop, box.clientHeight);
  };

  useImperativeHandle(ref, () => ({
    scrollToPage(page, smooth = false) {
      const box = scroller.current;
      if (!box) return;
      const top = offsets[Math.min(Math.max(0, page), offsets.length - 1)] ?? 0;
      anchor.current = { page, within: 0 };
      const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      box.scrollTo({ top, behavior: smooth && !reduce ? 'smooth' : 'auto' });
    },
    scrollBy(fraction) {
      scroller.current?.scrollBy({ top: fraction * (scroller.current.clientHeight || 0) });
    },
  }), [offsets]);

  const pages: number[] = [];
  for (let page = range.start; page < Math.min(range.end, pageCount); page++) pages.push(page);

  return (
    <div ref={scroller} className="h-full w-full overflow-y-auto overflow-x-hidden [overflow-anchor:none]" onScroll={onScroll}>
      <div className="relative mx-auto" style={{ height: total, width: Math.max(width, 1) }}>
        {pages.map((page) => (
          <div key={page} className="absolute inset-x-0 flex justify-center" style={{ top: offsets[page] }}>
            <PageImage
              page={page}
              src={mangaUrl.page(volumeId, page)}
              alt={t('pageAlt', { page: page + 1 })}
              size={sizes[page]}
              failedLabel={t('pageFailed')}
              onNaturalSize={(natural) => onNaturalSize(page, natural)}
            >
              {overlay?.(page, sizes[page])}
            </PageImage>
          </div>
        ))}
      </div>
    </div>
  );
});
