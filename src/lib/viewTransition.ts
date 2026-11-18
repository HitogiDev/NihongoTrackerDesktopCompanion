import { flushSync } from 'react-dom';

/**
 * View Transitions where the webview has them (WebView2 does; WebKitGTK and
 * WKWebView only in recent versions), never with reduced motion. Without
 * them the screen that appears fades in on its own (`.manga-fade-in`).
 */
type TransitionDocument = Document & {
  startViewTransition?: (update: () => Promise<void> | void) => { finished: Promise<void> };
};

export function canViewTransition(doc: Document = document, win: Pick<Window, 'matchMedia'> = window): boolean {
  if (typeof (doc as TransitionDocument).startViewTransition !== 'function') return false;
  return !win.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

/** The screen just appeared inside a view transition (so it need not fade). */
export function inViewTransition(doc: Document = document): boolean {
  return doc.documentElement.dataset.viewTransition === 'on';
}

/** Resolves once `ready()` holds (the new screen drew what morphs), or after
 *  `timeout` ms: a screen still loading just cross-fades. */
export function waitFor(ready: () => boolean, timeout: number): Promise<void> {
  return new Promise((resolve) => {
    const start = performance.now();
    const check = () => {
      if (ready() || performance.now() - start >= timeout) resolve();
      else requestAnimationFrame(check);
    };
    check();
  });
}

/**
 * Runs `update` (a navigation, a state change) inside a view transition when
 * there is one, and plainly otherwise. `ready` says when the new state is on
 * screen; the old one stays frozen until then, at most `timeout` ms.
 */
export function withViewTransition(update: () => void, ready: () => boolean = () => true, timeout = 300): void {
  const doc = document as TransitionDocument;
  if (!canViewTransition(doc)) {
    update();
    return;
  }
  // The screen that appears asks this to skip its own fade.
  const root = doc.documentElement;
  try {
    root.dataset.viewTransition = 'on';
    const transition = doc.startViewTransition!(async () => {
      flushSync(update);
      await waitFor(ready, timeout);
    });
    void transition.finished.finally(() => { delete root.dataset.viewTransition; });
  } catch {
    delete root.dataset.viewTransition;
    update();
  }
}
