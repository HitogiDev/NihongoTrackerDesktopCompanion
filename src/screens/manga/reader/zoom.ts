/**
 * Zoom and pan of a page in the paged reader, as pure math: a scale and a
 * translation applied to a layer the size of the window, with its origin at
 * the top left (`transform: translate(x, y) scale(scale)`).
 */

export interface Zoom {
  scale: number;
  x: number;
  y: number;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export const NO_ZOOM: Zoom = { scale: 1, x: 0, y: 0 };
export const MIN_SCALE = 1;
export const MAX_SCALE = 6;
/** Closer to 1 than this is 1: no stray 1.003 that keeps the page "zoomed". */
const SNAP = 0.01;

export function isZoomed(zoom: Zoom): boolean {
  return zoom.scale > 1 + SNAP;
}

/**
 * Keeps the page in view: along each axis, a page drawn smaller than the
 * window stays where it is at scale 1 scaled around its center, and a
 * larger one cannot be dragged past its edges. `content` is the page's box
 * at scale 1 (what is visible of it), `view` the window.
 */
export function clampZoom(zoom: Zoom, content: Rect, view: { width: number; height: number }): Zoom {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, zoom.scale));
  if (scale <= 1 + SNAP) return NO_ZOOM;
  const axis = (offset: number, start: number, length: number, viewLength: number) => {
    const drawn = length * scale;
    if (drawn <= viewLength) {
      // Centered where the page's center was.
      return start + length / 2 - (start + length / 2) * scale;
    }
    // Its far edge no further in than the window's, its near edge no further out.
    const most = -start * scale;
    const least = viewLength - (start + length) * scale;
    return Math.min(most, Math.max(least, offset));
  };
  return {
    scale,
    x: axis(zoom.x, content.left, content.width, view.width),
    y: axis(zoom.y, content.top, content.height, view.height),
  };
}

/** Zooms by `factor` keeping the point under the cursor (`at`, window px) still. */
export function zoomAt(zoom: Zoom, factor: number, at: { x: number; y: number }, content: Rect, view: { width: number; height: number }): Zoom {
  const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, zoom.scale * factor));
  const ratio = scale / zoom.scale;
  return clampZoom({ scale, x: at.x - (at.x - zoom.x) * ratio, y: at.y - (at.y - zoom.y) * ratio }, content, view);
}

/** Moves the zoomed page by `dx`, `dy` window pixels. */
export function panBy(zoom: Zoom, dx: number, dy: number, content: Rect, view: { width: number; height: number }): Zoom {
  if (!isZoomed(zoom)) return zoom;
  return clampZoom({ ...zoom, x: zoom.x + dx, y: zoom.y + dy }, content, view);
}

/**
 * How much one wheel event zooms. A pinch on a touchpad arrives as a wheel
 * event with Ctrl and a small pixel delta, a mouse notch as about 100 px (or
 * 3 lines): both turn smoothly into the same curve, a notch about 20 %.
 */
export function wheelFactor(deltaY: number, deltaMode: number): number {
  const pixels = deltaMode === 1 ? deltaY * 40 : deltaMode === 2 ? deltaY * 800 : deltaY;
  // A notch never more than 2x, however the device reports it.
  const step = Math.max(-300, Math.min(300, pixels));
  return Math.exp(-step * 0.0018);
}

/** How far one Ctrl+arrow moves a zoomed page: a fifth of the window. */
export const ARROW_PAN = 0.2;

/**
 * What Ctrl+arrow moves the page by, in window pixels: the arrow says what to
 * look at, so the page goes the other way (Ctrl+→ shows what is to the
 * right). `null` for any other key.
 */
export function arrowPan(key: string, view: { width: number; height: number }): { dx: number; dy: number } | null {
  const x = view.width * ARROW_PAN;
  const y = view.height * ARROW_PAN;
  switch (key) {
    case 'ArrowLeft': return { dx: x, dy: 0 };
    case 'ArrowRight': return { dx: -x, dy: 0 };
    case 'ArrowUp': return { dx: 0, dy: y };
    case 'ArrowDown': return { dx: 0, dy: -y };
    default: return null;
  }
}

/** How quickly an animated zoom closes on its target (ms to ~63 %). */
export const ZOOM_EASE_MS = 70;

/**
 * One frame of an animated zoom: `dt` ms closer to `target`, eased. Scale and
 * translation move together linearly, so a page point that both ends keep
 * under the cursor stays there the whole way; and both ends being inside the
 * bounds keeps every frame inside them (the bounds are linear). `done` when
 * it is there.
 */
export function approach(current: Zoom, target: Zoom, dt: number): { zoom: Zoom; done: boolean } {
  const k = 1 - Math.exp(-Math.max(0, dt) / ZOOM_EASE_MS);
  const next = {
    scale: current.scale + (target.scale - current.scale) * k,
    x: current.x + (target.x - current.x) * k,
    y: current.y + (target.y - current.y) * k,
  };
  const close = Math.abs(next.scale - target.scale) < 0.002 && Math.abs(next.x - target.x) < 0.5 && Math.abs(next.y - target.y) < 0.5;
  return close ? { zoom: target, done: true } : { zoom: next, done: false };
}

/** The CSS transform of a zoom. */
export function zoomTransform(zoom: Zoom): string {
  return isZoomed(zoom) ? `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.scale})` : '';
}
