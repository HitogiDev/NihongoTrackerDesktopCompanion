import type { PageFit, ReaderLayout, ReadingDirection } from '../../../lib/types';

/**
 * The pages shown together, in reading order. One page per spread unless the
 * layout is double: then pages pair up, the first one alone when the cover
 * stands by itself, and a wide page (a two-page scan) always alone, which
 * restarts the pairing after it, as in the printed book.
 */
export function buildSpreads(
  pageCount: number,
  layout: ReaderLayout,
  coverAlone: boolean,
  isWide: (page: number) => boolean = () => false,
): number[][] {
  if (pageCount <= 0) return [];
  if (layout !== 'double') return Array.from({ length: pageCount }, (_, page) => [page]);
  const spreads: number[][] = [];
  let page = 0;
  if (coverAlone) spreads.push([page++]);
  while (page < pageCount) {
    if (isWide(page) || page + 1 >= pageCount || isWide(page + 1)) {
      spreads.push([page++]);
    } else {
      spreads.push([page, page + 1]);
      page += 2;
    }
  }
  return spreads;
}

/** The spread that holds `page`, or the last one past the end. */
export function spreadOf(spreads: number[][], page: number): number {
  const index = spreads.findIndex((spread) => spread.includes(page));
  if (index >= 0) return index;
  return page <= 0 ? 0 : Math.max(0, spreads.length - 1);
}

/**
 * Pages left to right on screen. Right-to-left books put the first page of a
 * spread on the right.
 */
export function onScreen(spread: number[], direction: ReadingDirection): number[] {
  return direction === 'rtl' ? [...spread].reverse() : spread;
}

export type ReaderAction =
  | 'next' | 'previous' | 'first' | 'last' | 'fullscreen' | 'escape'
  /** The strip: a screen down or up, or a short step. */
  | 'screenDown' | 'screenUp' | 'stepDown' | 'stepUp';

/**
 * What a key does. Arrows follow the page: in a right-to-left book the next
 * page is on the left. In the strip, the keys that turn pages scroll instead.
 */
export function keyAction(event: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'metaKey' | 'altKey'>, direction: ReadingDirection, layout: ReaderLayout): ReaderAction | null {
  if (event.ctrlKey || event.metaKey || event.altKey) return null;
  const vertical = layout === 'vertical';
  switch (event.key) {
    case 'ArrowLeft': return vertical ? null : direction === 'rtl' ? 'next' : 'previous';
    case 'ArrowRight': return vertical ? null : direction === 'rtl' ? 'previous' : 'next';
    case 'ArrowDown': return vertical ? 'stepDown' : 'next';
    case 'ArrowUp': return vertical ? 'stepUp' : 'previous';
    case 'PageDown': return vertical ? 'screenDown' : 'next';
    case 'PageUp': return vertical ? 'screenUp' : 'previous';
    case ' ':
      if (vertical) return event.shiftKey ? 'screenUp' : 'screenDown';
      return event.shiftKey ? 'previous' : 'next';
    case 'Home': return 'first';
    case 'End': return 'last';
    case 'f':
    case 'F': return 'fullscreen';
    case 'Escape': return 'escape';
    default: return null;
  }
}

/** A click on the page: the outer thirds turn it, the middle shows the controls. */
export function clickAction(x: number, width: number, direction: ReadingDirection): 'next' | 'previous' | 'toggle' {
  const ratio = width > 0 ? x / width : 0.5;
  if (ratio < 0.35) return direction === 'rtl' ? 'next' : 'previous';
  if (ratio > 0.65) return direction === 'rtl' ? 'previous' : 'next';
  return 'toggle';
}

/** The pages to decode ahead of time: the spreads around the current one. */
export function preloadPages(spreads: number[][], current: number, around = 2): number[] {
  const pages: number[] = [];
  for (let offset = 1; offset <= around; offset++) {
    for (const index of [current + offset, current - offset]) {
      if (index >= 0 && index < spreads.length) pages.push(...spreads[index]);
    }
  }
  return pages;
}

export type Size = { width: number; height: number };
/** A page whose size is not known yet is drawn as a usual manga page. */
export const DEFAULT_PAGE: Size = { width: 2, height: 3 };

/**
 * The size each page of a spread is drawn at, in CSS pixels.
 *
 * - `height`: the spread is as tall as the window, unless that would make it
 *   wider than the window, in which case it is as wide.
 * - `width`: the spread is as wide as the window (it may scroll down).
 * - `original`: pixel for pixel.
 *
 * Pages of a spread share a height, so they meet edge to edge whatever their
 * own sizes.
 */
export function fitSpread(pages: Size[], viewport: Size, fit: PageFit): Size[] {
  const sane = pages.map((page) => (page.width > 0 && page.height > 0 ? page : DEFAULT_PAGE));
  if (fit === 'original') return sane;
  const ratio = sane.reduce((sum, page) => sum + page.width / page.height, 0);
  if (ratio <= 0 || viewport.width <= 0 || viewport.height <= 0) return sane;
  const byWidth = viewport.width / ratio;
  const height = fit === 'width' ? byWidth : Math.min(viewport.height, byWidth);
  return sane.map((page) => ({ width: Math.round((page.width / page.height) * height), height: Math.round(height) }));
}
