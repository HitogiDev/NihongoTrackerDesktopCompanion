import { describe, expect, it } from 'vitest';
import { approach, clampZoom, isZoomed, MAX_SCALE, NO_ZOOM, panBy, wheelFactor, zoomAt, zoomTransform } from './zoom';

// A portrait page centered in a wide window, at scale 1.
const view = { width: 1600, height: 900 };
const page = { left: 500, top: 0, width: 600, height: 900 };
const where = (zoom: { scale: number; x: number }, x: number) => x * zoom.scale + zoom.x;

describe('zoom and pan', () => {
  it('keeps the point under the cursor still while zooming', () => {
    // At 3x the page (1800 px) is wider than the window on both axes.
    const at = { x: 800, y: 300 };
    const zoomed = zoomAt(NO_ZOOM, 3, at, page, view);
    expect(zoomed.scale).toBe(3);
    // The page point that was under the cursor is still under it.
    expect(where(zoomed, 800)).toBeCloseTo(800);
    expect(300 * zoomed.scale + zoomed.y).toBeCloseTo(300);
    // Zooming in again around another point keeps that one still too.
    const again = zoomAt(zoomed, 1.5, { x: 700, y: 500 }, page, view);
    const pageX = (700 - zoomed.x) / zoomed.scale;
    expect(where(again, pageX)).toBeCloseTo(700);
  });

  it('never zooms out past the fitted page nor in past the limit', () => {
    expect(zoomAt(NO_ZOOM, 0.5, { x: 800, y: 450 }, page, view)).toEqual(NO_ZOOM);
    expect(zoomAt(NO_ZOOM, 100, { x: 800, y: 450 }, page, view).scale).toBe(MAX_SCALE);
    // Nearly 1 is 1.
    expect(isZoomed(zoomAt(NO_ZOOM, 1.005, { x: 800, y: 450 }, page, view))).toBe(false);
  });

  it('keeps a zoomed page over the window: its edges never come inside', () => {
    const zoomed = zoomAt(NO_ZOOM, 3, { x: 800, y: 450 }, page, view);
    const right = panBy(zoomed, -100000, 0, page, view);
    // Dragged as far left as it goes: its right edge sits on the window's.
    expect(where(right, page.left + page.width)).toBeCloseTo(view.width);
    const left = panBy(zoomed, 100000, 100000, page, view);
    expect(where(left, page.left)).toBeCloseTo(0);
    expect(page.top * left.scale + left.y).toBeCloseTo(0);
  });

  it('centers an axis that still fits the window', () => {
    // 1.5x: the page is 900 px wide, the window 1600, so it stays centered.
    const zoomed = clampZoom({ scale: 1.5, x: 9999, y: 0 }, page, view);
    expect(where(zoomed, page.left + page.width / 2)).toBeCloseTo(page.left + page.width / 2);
  });

  it('does not pan a page that is not zoomed', () => {
    expect(panBy(NO_ZOOM, 50, 50, page, view)).toBe(NO_ZOOM);
  });

  it('zooms a pinch gently and a mouse notch by about a fifth, in the right direction', () => {
    expect(wheelFactor(-100, 0)).toBeGreaterThan(1.15);
    expect(wheelFactor(-100, 0)).toBeLessThan(1.25);
    expect(wheelFactor(100, 0)).toBeLessThan(1);
    expect(wheelFactor(-3, 1)).toBeCloseTo(wheelFactor(-120, 0));
    const pinch = wheelFactor(-4, 0);
    expect(pinch).toBeGreaterThan(1);
    expect(pinch).toBeLessThan(1.01);
    // A wildly large delta still zooms one sane step.
    expect(wheelFactor(-100000, 0)).toBeLessThan(2);
  });

  it('animates towards the target keeping the anchored point still, and arrives', () => {
    const at = { x: 800, y: 300 };
    const target = zoomAt(NO_ZOOM, 3, at, page, view);
    let current = NO_ZOOM;
    let frames = 0;
    let at15 = NO_ZOOM;
    for (let done = false; !done && frames < 200; frames++) {
      const step = approach(current, target, 16);
      current = step.zoom;
      done = step.done;
      if (frames === 14) at15 = current;
      // Every frame: the point under the cursor has not moved.
      expect(where(current, 800)).toBeCloseTo(800, 4);
      expect(300 * current.scale + current.y).toBeCloseTo(300, 4);
    }
    expect(current).toBe(target);
    // Nearly there in a quarter of a second at 60 fps, and it does end.
    expect((at15.scale - 1) / (target.scale - 1)).toBeGreaterThan(0.95);
    expect(frames).toBeLessThan(60);
    expect(frames).toBeGreaterThan(5);
  });

  it('writes no transform when not zoomed', () => {
    expect(zoomTransform(NO_ZOOM)).toBe('');
    expect(zoomTransform({ scale: 2, x: -10, y: 5 })).toBe('translate(-10px, 5px) scale(2)');
  });
});
