import { useCallback, useEffect, useRef, type RefObject } from 'react';
import type { PageTransition, ReadingDirection } from '../../../lib/types';
import { swipeForward, swipeProgress, swipeTurns, SWIPE_SLOP, turnSide, type Corner } from './turn';
import type { TurnController } from './turnController';

/** What a swipe must leave alone: the bars, dialogs and the editor. */
const OWN_INPUT = '[data-reader-chrome], dialog, [role="dialog"], [data-edit-layer]';
/** What on the top bar is a control; the rest of it is only over the page. */
const CONTROL = 'button, a, input, select, textarea, [role="button"], [role="slider"], [role="menu"], [role="radio"]';

/**
 * A swipe may start here. The top bar comes up with any move of the mouse,
 * right over the page's top corners: a press on it that is not on one of
 * its controls takes the corner below it.
 */
function swipeStartsOn(target: HTMLElement | null): boolean {
  const own = target?.closest(OWN_INPUT);
  if (!own) return true;
  return own.hasAttribute('data-reader-topbar') && !target?.closest(CONTROL);
}
/** A touchpad swipe has ended when its wheel events stop this long. */
const WHEEL_END_MS = 180;

export interface SwipeOptions {
  /** Off in the strip, while editing, and with the threshold at 0. */
  enabled: boolean;
  /** % of the window's width; 0 never turns. */
  threshold: number;
  transition: PageTransition;
  direction: ReadingDirection;
  /** A mouse drag that starts on a text block selects text instead. */
  textSelectable: boolean;
  /** A zoomed page pans instead. */
  isZoomed: () => boolean;
  /** The spread on screen, and the one a turn that way would bring (null: none, or the end). */
  current: () => number[];
  neighbour: (forward: boolean) => number[] | null;
  /** Leaving the threshold behind: turn. */
  onTurn: (forward: boolean) => void;
  turns: TurnController;
}

/**
 * Swiping turns the page: a finger, a pen or a mouse dragged sideways, or two
 * fingers on a touchpad (which Windows hands over as horizontal wheel
 * events). The turn follows it while it moves, and letting go past the
 * threshold finishes it; short of it, the page goes back.
 */
export function useSwipe(root: RefObject<HTMLElement | null>, options: SwipeOptions): { consumeClick: () => boolean } {
  const latest = useRef(options);
  latest.current = options;
  const swallowClick = useRef(false);

  useEffect(() => {
    const element = root.current;
    if (!element) return;
    let dx = 0;
    let dy = 0;
    let engaged = false;
    /** The corner taken: the one nearer the press. A touchpad has no press: the bottom one. */
    let corner: Corner = 'bottom';
    /** A hand holds the corner (finger, pen, mouse), so it follows it up and down too. */
    let held = false;

    /** Halfway down the pages on screen: above it a press takes the top corner. */
    const pagesMiddle = () => {
      let top = Infinity;
      let bottom = -Infinity;
      element.querySelectorAll<HTMLElement>('[data-page]').forEach((page) => {
        const rect = page.getBoundingClientRect();
        top = Math.min(top, rect.top);
        bottom = Math.max(bottom, rect.bottom);
      });
      return Number.isFinite(top) ? (top + bottom) / 2 : window.innerHeight / 2;
    };

    /** The turn on screen follows `dx`. */
    const follow = () => {
      const o = latest.current;
      if (o.transition === 'none' || dx === 0) {
        o.turns.clear();
        return;
      }
      const forward = swipeForward(dx, o.direction);
      const arriving = o.neighbour(forward);
      const leaving = o.current();
      if (!arriving || leaving.length === 0) {
        o.turns.clear();
        return;
      }
      const side = turnSide(forward, o.direction);
      let sheetWidth = 0;
      element.querySelectorAll<HTMLElement>('[data-page]').forEach((page) => { sheetWidth += page.getBoundingClientRect().width; });
      o.turns.drag({
        kind: o.transition,
        side,
        corner,
        leaving,
        arriving,
        progress: swipeProgress(o.transition, dx, window.innerWidth, sheetWidth || window.innerWidth),
        hand: held ? { x: dx, y: dy } : null,
        released: null,
      });
    };
    /** The swipe is over: turn, or put the page back. */
    const finish = () => {
      const o = latest.current;
      const turns = swipeTurns(dx, window.innerWidth, o.threshold);
      if (turns) {
        o.turns.release(true);
        o.onTurn(swipeForward(dx, o.direction));
      } else {
        o.turns.release(false);
      }
      dx = 0;
      dy = 0;
      engaged = false;
    };

    // ── pointer: finger, pen, mouse ─────────────────────────────────
    let press: { id: number; x: number; y: number } | null = null;
    const onPointerDown = (event: PointerEvent) => {
      const o = latest.current;
      swallowClick.current = false;
      press = null;
      if (!o.enabled || o.threshold <= 0 || o.isZoomed()) return;
      if (event.button !== 0 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
      const target = event.target as HTMLElement | null;
      if (!swipeStartsOn(target)) return;
      if (event.pointerType === 'mouse' && o.textSelectable && target?.closest('[data-text-block]')) return;
      press = { id: event.pointerId, x: event.clientX, y: event.clientY };
      dx = 0;
      dy = 0;
      engaged = false;
      held = true;
      corner = event.clientY < pagesMiddle() ? 'top' : 'bottom';
    };
    const onPointerMove = (event: PointerEvent) => {
      if (!press || press.id !== event.pointerId) return;
      const moveX = event.clientX - press.x;
      const moveY = event.clientY - press.y;
      if (!engaged) {
        if (Math.abs(moveY) > SWIPE_SLOP && Math.abs(moveY) > Math.abs(moveX)) {
          // Scrolling or selecting, not swiping.
          press = null;
          return;
        }
        if (Math.abs(moveX) <= SWIPE_SLOP) return;
        engaged = true;
        try {
          element.setPointerCapture(event.pointerId);
        } catch {
          // A pointer already gone: the swipe still ends with its pointerup.
        }
        window.getSelection()?.removeAllRanges();
      }
      event.preventDefault();
      dx = moveX;
      dy = moveY;
      follow();
    };
    const onPointerUp = (event: PointerEvent) => {
      if (!press || press.id !== event.pointerId) return;
      press = null;
      if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
      if (!engaged) return;
      // The click that ends a swipe is not a page turn of its own.
      swallowClick.current = true;
      if (event.type === 'pointercancel') dx = 0;
      finish();
    };

    // ── touchpad: two fingers sideways ──────────────────────────────
    let wheelEnd: number | null = null;
    /** The rest of a swipe that already turned: no second turn from it. */
    let spent = false;
    const onWheel = (event: WheelEvent) => {
      const o = latest.current;
      if (event.ctrlKey || event.deltaMode !== 0 || Math.abs(event.deltaX) <= Math.abs(event.deltaY)) return;
      if (!o.enabled || o.threshold <= 0 || o.isZoomed() || press) return;
      if ((event.target as HTMLElement | null)?.closest(OWN_INPUT)) return;
      event.preventDefault();
      if (wheelEnd !== null) window.clearTimeout(wheelEnd);
      wheelEnd = window.setTimeout(() => {
        wheelEnd = null;
        if (spent) spent = false;
        else finish();
      }, WHEEL_END_MS);
      if (spent) return;
      // Fingers to the right scroll the content to the left: deltaX < 0.
      dx -= event.deltaX;
      held = false;
      corner = 'bottom';
      engaged = true;
      follow();
      // Far enough: turn now, rather than when the fingers stop.
      if (swipeTurns(dx, window.innerWidth, o.threshold)) {
        finish();
        spent = true;
      }
    };

    element.addEventListener('pointerdown', onPointerDown);
    element.addEventListener('pointermove', onPointerMove);
    element.addEventListener('pointerup', onPointerUp);
    element.addEventListener('pointercancel', onPointerUp);
    element.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      element.removeEventListener('pointerdown', onPointerDown);
      element.removeEventListener('pointermove', onPointerMove);
      element.removeEventListener('pointerup', onPointerUp);
      element.removeEventListener('pointercancel', onPointerUp);
      element.removeEventListener('wheel', onWheel);
      if (wheelEnd !== null) window.clearTimeout(wheelEnd);
      latest.current.turns.clear();
    };
  }, [root]);

  const consumeClick = useCallback(() => {
    const swallowed = swallowClick.current;
    swallowClick.current = false;
    return swallowed;
  }, []);
  return { consumeClick };
}
