import type { MouseEvent as ReactMouseEvent } from 'react';
import type { NavigateFunction } from 'react-router-dom';
import { withViewTransition } from '../../lib/viewTransition';

/**
 * A cover in the library grows into the series page and shrinks back into
 * the grid: one `view-transition-name` (`manga-cover`), on the clicked card
 * before and on the page's cover after. Only one element may carry it at a
 * time, so the cards get it by hand, right before a transition, and lose it
 * afterwards. Where the webview has no View Transitions the series page just
 * fades in (`manga-fade-in`).
 */
export const COVER_TRANSITION = 'manga-cover';

function cardCover(seriesId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-series-cover="${CSS.escape(seriesId)}"]`);
}

function heroCover(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[data-hero-cover]');
}

/** The picture is decoded and on screen, so the snapshot shows it. */
function pictureReady(element: HTMLElement | null): boolean {
  const image = element?.querySelector('img');
  return Boolean(image?.complete && image.naturalWidth > 0 && getComputedStyle(image).opacity === '1');
}

function clearNames() {
  for (const element of document.querySelectorAll<HTMLElement>('[data-series-cover]')) element.style.viewTransitionName = '';
}

/** A click on a card: to its series page, the cover morphing on the way. */
export function openSeries(event: ReactMouseEvent<HTMLAnchorElement>, seriesId: string, navigate: NavigateFunction) {
  // A middle click or a modifier keeps the link's own behaviour.
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  clearNames();
  const card = cardCover(seriesId);
  if (card) card.style.viewTransitionName = COVER_TRANSITION;
  withViewTransition(() => navigate(`/manga/${seriesId}`), () => pictureReady(heroCover()), 350);
}

/** Back to the library, the cover returning to its card. */
export function backToLibrary(seriesId: string, navigate: NavigateFunction, state?: unknown) {
  clearNames();
  withViewTransition(
    () => navigate('/manga', { state }),
    () => {
      const card = cardCover(seriesId);
      if (!card) return false;
      card.style.viewTransitionName = COVER_TRANSITION;
      return pictureReady(card);
    },
    350,
  );
}
