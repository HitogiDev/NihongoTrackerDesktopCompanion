import { useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit, PageTransition, ReadingDirection } from '../../../lib/types';
import { fitSpread, type Size } from './layout';
import { PageImage } from './PageImage';
import { turnSide } from './turn';
import type { TurnController } from './turnController';
import { TurnLayer } from './TurnLayer';

/** How long a page turn waits for the next spread to decode before showing
 *  it anyway, with its spinner. Preloaded spreads are ready at once. */
const DECODE_WAIT_MS = 250;

/**
 * One spread (one or two pages) at a time. Turning the page keeps the old
 * spread up until the new one is decoded (`decode()`, at most
 * `DECODE_WAIT_MS`), so a page never paints in halves, and then turns it with
 * the chosen transition (`TurnLayer`), unless a swipe is already turning it.
 * A spread that changes without a turn (another layout) fades in briefly
 * (`manga-page-in`, off with reduced motion). A new spread starts at the top
 * when the fit makes it taller than the window.
 */
export function PagedView({ volumeId, spread, direction, fit, viewport, sizeOf, onNaturalSize, overlay, transition, turns, turnKey, backgroundClass }: {
  volumeId: string;
  spread: number[];
  direction: ReadingDirection;
  fit: PageFit;
  viewport: Size;
  sizeOf: (page: number) => Size;
  onNaturalSize: (page: number, size: Size) => void;
  /** What to draw over a page (the text layer), given its drawn size. */
  overlay?: (page: number, size: Size) => ReactNode;
  transition: PageTransition;
  turns: TurnController;
  /** Changes when a new spread is not a page turn (another volume or layout). */
  turnKey: string;
  backgroundClass: string;
}) {
  const { t } = useTranslation('mangaReader');
  const scroller = useRef<HTMLDivElement>(null);
  const wanted = spread.join('-');
  const [pages, setPages] = useState(spread);
  const key = pages.join('-');
  const shownTurnKey = useRef(turnKey);
  const latest = useRef({ transition, direction, turns });
  latest.current = { transition, direction, turns };
  useEffect(() => {
    if (wanted === key) return;
    let alive = true;
    const leaving = pages;
    const turning = shownTurnKey.current === turnKey;
    const turn = () => {
      const { transition: kind, direction: reading, turns: controller } = latest.current;
      const current = controller.get();
      // A swipe brought it: its turn is already on screen.
      if (current && current.phase !== 'cancel' && current.arriving.join('-') === wanted) return;
      if (current?.phase === 'drag') return;
      controller.clear();
      if (kind === 'none' || !turning || leaving.length === 0 || reducedMotion()) return;
      // A click or a key turns it by its bottom corner, as a hand would.
      controller.play({ kind, side: turnSide(spread[0] > leaving[0], reading), corner: 'bottom', leaving, arriving: spread });
    };
    const decoded = spread.map((page) => {
      const image = new Image();
      image.src = mangaUrl.page(volumeId, page);
      return image.decode().catch(() => undefined);
    });
    const waited = new Promise((resolve) => window.setTimeout(resolve, DECODE_WAIT_MS));
    void Promise.race([Promise.all(decoded), waited]).then(() => {
      if (!alive) return;
      turn();
      setPages(spread);
    });
    return () => { alive = false; };
    // `spread` is new on every render; `wanted` is what it says.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wanted, key, volumeId]);
  useEffect(() => { shownTurnKey.current = turnKey; }, [turnKey]);
  // In reading order, laid out right to left for an RTL book: Tab and screen
  // readers then meet the first page first.
  const sizes = fitSpread(pages.map(sizeOf), viewport, fit);

  useEffect(() => {
    scroller.current?.scrollTo({ top: 0, left: direction === 'rtl' ? scroller.current.scrollWidth : 0 });
  }, [key, direction]);

  return (
    <div ref={scroller} className={fit === 'height' ? 'h-full w-full overflow-hidden' : 'h-full w-full overflow-auto'}>
      <div key={key} className={`${transition === 'none' ? '' : 'manga-page-in'} flex min-h-full min-w-full w-max items-center justify-center ${direction === 'rtl' ? 'flex-row-reverse' : ''}`}>
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
      <TurnLayer controller={turns} volumeId={volumeId} viewport={viewport} fit={fit} direction={direction} sizeOf={sizeOf} backgroundClass={backgroundClass} />
    </div>
  );
}

function reducedMotion(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}
