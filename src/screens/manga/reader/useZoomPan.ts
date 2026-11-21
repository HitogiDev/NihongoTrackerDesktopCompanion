import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type RefObject } from 'react';
import { approach, clampZoom, isZoomed, NO_ZOOM, panBy, wheelFactor, zoomAt, zoomTransform, type Rect, type Zoom } from './zoom';

/** What the clicks and wheels of the reader must leave alone. */
const OWN_INPUT = '[data-reader-chrome], dialog, [role="dialog"]';
/** Ctrl+= and Ctrl+-: one step. */
export const KEY_STEP = 1.25;

export interface ZoomPan {
  /** The page is zoomed: a plain wheel pans instead of turning the page. */
  zoomed: boolean;
  /** `grab` while Ctrl is held over a zoomed page, `grabbing` while dragging. */
  cursor: 'grab' | 'grabbing' | null;
  /** The click that ends a Ctrl+drag (or a Ctrl+click) is the pan's, not a page turn. */
  consumeClick: () => boolean;
  reset: () => void;
  /** Zooms around the center of the window (the keyboard). */
  zoomBy: (factor: number) => void;
  /** Moves a zoomed page by window pixels, animated (Ctrl+arrows). */
  panStep: (dx: number, dy: number) => void;
  subscribe: (listener: () => void) => () => void;
  getScale: () => number;
}

/**
 * Zoom and pan of the paged reader. Ctrl+wheel zooms around the pointer (a
 * touchpad pinch arrives as exactly that: a wheel with Ctrl); Ctrl+drag pans;
 * once zoomed, a plain wheel or a two-finger swipe pans too. The transform is
 * written on `layer` directly, never through React state, so zooming does not
 * redraw the reader. `resetKey` changing (another spread, layout or fit) goes
 * back to the fitted page.
 */
export function useZoomPan(root: RefObject<HTMLElement | null>, layer: RefObject<HTMLElement | null>, { enabled, resetKey }: { enabled: boolean; resetKey: string }): ZoomPan {
  const zoom = useRef<Zoom>(NO_ZOOM);
  const listeners = useRef(new Set<() => void>());
  const [zoomed, setZoomed] = useState(false);
  const [cursor, setCursor] = useState<ZoomPan['cursor']>(null);
  const ctrlHeld = useRef(false);
  const drag = useRef<{ id: number; x: number; y: number } | null>(null);
  const swallowClick = useRef(false);

  const view = () => ({ width: window.innerWidth, height: window.innerHeight });

  /** The pages' box at scale 1, cut to the window: what panning may show. */
  const content = useCallback((): Rect => {
    const pages = layer.current?.querySelectorAll<HTMLElement>('[data-page]');
    const { scale, x, y } = zoom.current;
    let box: { left: number; top: number; right: number; bottom: number } | null = null;
    pages?.forEach((page) => {
      const rect = page.getBoundingClientRect();
      const left = (rect.left - x) / scale;
      const top = (rect.top - y) / scale;
      const right = (rect.right - x) / scale;
      const bottom = (rect.bottom - y) / scale;
      box = box
        ? { left: Math.min(box.left, left), top: Math.min(box.top, top), right: Math.max(box.right, right), bottom: Math.max(box.bottom, bottom) }
        : { left, top, right, bottom };
    });
    const { width, height } = view();
    const found = box as { left: number; top: number; right: number; bottom: number } | null;
    if (!found) return { left: 0, top: 0, width, height };
    const left = Math.max(0, found.left);
    const top = Math.max(0, found.top);
    return { left, top, width: Math.max(1, Math.min(width, found.right) - left), height: Math.max(1, Math.min(height, found.bottom) - top) };
  }, [layer]);

  const apply = useCallback((next: Zoom) => {
    if (next === zoom.current) return;
    zoom.current = next;
    if (layer.current) {
      layer.current.style.transformOrigin = '0 0';
      layer.current.style.transform = zoomTransform(next);
    }
    const now = isZoomed(next);
    setZoomed(now);
    if (!now) setCursor(drag.current ? 'grabbing' : null);
    else if (ctrlHeld.current && !drag.current) setCursor('grab');
    listeners.current.forEach((listener) => listener());
  }, [layer]);

  // Zooming is animated towards where it is going (`approach`): a mouse
  // notch, and a pinch on touchpads that Windows hands over as Ctrl+wheel
  // notches of ±100 a few times a second (measured), would otherwise jump.
  // Panning is not: it follows the fingers or the pointer as they move.
  const target = useRef<Zoom | null>(null);
  const frame = useRef<number | null>(null);
  const lastFrame = useRef(0);
  const stopAnimation = useCallback(() => {
    if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null;
    target.current = null;
  }, []);
  const animateTo = useCallback((next: Zoom) => {
    target.current = next;
    if (frame.current !== null) return;
    lastFrame.current = performance.now();
    const tick = (now: number) => {
      const goal = target.current;
      if (!goal) {
        frame.current = null;
        return;
      }
      const step = approach(zoom.current, goal, now - lastFrame.current);
      lastFrame.current = now;
      apply(step.zoom);
      if (step.done) {
        frame.current = null;
        target.current = null;
      } else {
        frame.current = requestAnimationFrame(tick);
      }
    };
    frame.current = requestAnimationFrame(tick);
  }, [apply]);
  /** Where the zoom is going: a new step builds on that, not on this frame. */
  const heading = () => target.current ?? zoom.current;
  /** Before moving the page by hand: the zoom lands where it was going. */
  const settle = useCallback(() => {
    if (target.current) apply(target.current);
    stopAnimation();
  }, [apply, stopAnimation]);
  useEffect(() => stopAnimation, [stopAnimation]);

  const reset = useCallback(() => {
    stopAnimation();
    apply(NO_ZOOM);
  }, [apply, stopAnimation]);
  const zoomBy = useCallback((factor: number) => {
    const { width, height } = view();
    animateTo(zoomAt(heading(), factor, { x: width / 2, y: height / 2 }, content(), view()));
  }, [animateTo, content]);
  const panStep = useCallback((dx: number, dy: number) => {
    if (!isZoomed(heading())) return;
    animateTo(panBy(heading(), dx, dy, content(), view()));
  }, [animateTo, content]);

  // Another spread, layout or fit: back to the fitted page.
  useEffect(() => { reset(); }, [resetKey, enabled, reset]);

  useEffect(() => {
    const element = root.current;
    if (!element) return;

    const onWheel = (event: WheelEvent) => {
      if ((event.target as HTMLElement | null)?.closest(OWN_INPUT)) return;
      if (!enabled) {
        // No zooming the whole webview from the strip either.
        if (event.ctrlKey) event.preventDefault();
        return;
      }
      if (event.ctrlKey) {
        event.preventDefault();
        event.stopPropagation();
        animateTo(zoomAt(heading(), wheelFactor(event.deltaY, event.deltaMode), { x: event.clientX, y: event.clientY }, content(), view()));
        return;
      }
      if (!isZoomed(heading())) return;
      settle();
      // Zoomed: the wheel (or two fingers) moves the page, it does not turn it.
      event.preventDefault();
      event.stopPropagation();
      const unit = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? window.innerHeight : 1;
      // Shift+wheel is sideways on a mouse with only one wheel.
      const sideways = event.shiftKey && event.deltaX === 0;
      const dx = (sideways ? event.deltaY : event.deltaX) * unit;
      const dy = (sideways ? 0 : event.deltaY) * unit;
      apply(panBy(zoom.current, -dx, -dy, content(), view()));
    };

    const onPointerDown = (event: PointerEvent) => {
      const own = (event.target as HTMLElement | null)?.closest(OWN_INPUT);
      swallowClick.current = false;
      // On macOS Ctrl+click is a right click: ⌘+drag pans there too.
      if (!enabled || own || event.button !== 0 || !(event.ctrlKey || event.metaKey)) return;
      // Ctrl+click is never a page turn; Ctrl+drag pans (when zoomed).
      swallowClick.current = true;
      event.preventDefault();
      if (!isZoomed(heading())) return;
      settle();
      drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY };
      element.setPointerCapture(event.pointerId);
      setCursor('grabbing');
    };
    const onPointerMove = (event: PointerEvent) => {
      const current = drag.current;
      if (!current || current.id !== event.pointerId) return;
      const dx = event.clientX - current.x;
      const dy = event.clientY - current.y;
      drag.current = { ...current, x: event.clientX, y: event.clientY };
      apply(panBy(zoom.current, dx, dy, content(), view()));
    };
    const onPointerUp = (event: PointerEvent) => {
      if (drag.current?.id !== event.pointerId) return;
      drag.current = null;
      if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
      setCursor(ctrlHeld.current && isZoomed(zoom.current) ? 'grab' : null);
    };

    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Control') return;
      ctrlHeld.current = event.type === 'keydown';
      if (!drag.current) setCursor(ctrlHeld.current && isZoomed(zoom.current) ? 'grab' : null);
    };
    const onBlur = () => {
      ctrlHeld.current = false;
      if (!drag.current) setCursor(null);
    };
    // A smaller window may leave the page out of place.
    const onResize = () => {
      settle();
      apply(clampZoom(zoom.current, content(), view()));
    };

    element.addEventListener('wheel', onWheel, { passive: false });
    element.addEventListener('pointerdown', onPointerDown);
    element.addEventListener('pointermove', onPointerMove);
    element.addEventListener('pointerup', onPointerUp);
    element.addEventListener('pointercancel', onPointerUp);
    window.addEventListener('keydown', onKey);
    window.addEventListener('keyup', onKey);
    window.addEventListener('blur', onBlur);
    window.addEventListener('resize', onResize);
    return () => {
      element.removeEventListener('wheel', onWheel);
      element.removeEventListener('pointerdown', onPointerDown);
      element.removeEventListener('pointermove', onPointerMove);
      element.removeEventListener('pointerup', onPointerUp);
      element.removeEventListener('pointercancel', onPointerUp);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('keyup', onKey);
      window.removeEventListener('blur', onBlur);
      window.removeEventListener('resize', onResize);
    };
  // heading reads refs only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, enabled, apply, animateTo, settle, content]);

  const consumeClick = useCallback(() => {
    const swallowed = swallowClick.current;
    swallowClick.current = false;
    return swallowed;
  }, []);
  const subscribe = useCallback((listener: () => void) => {
    listeners.current.add(listener);
    return () => { listeners.current.delete(listener); };
  }, []);
  const getScale = useCallback(() => zoom.current.scale, []);

  return useMemo(() => ({ zoomed, cursor, consumeClick, reset, zoomBy, panStep, subscribe, getScale }), [zoomed, cursor, consumeClick, reset, zoomBy, panStep, subscribe, getScale]);
}

/** The zoom level, for the one small thing that shows it (`ZoomBadge`). */
export function useZoomScale(zoom: Pick<ZoomPan, 'subscribe' | 'getScale'>): number {
  return useSyncExternalStore(zoom.subscribe, zoom.getScale);
}
