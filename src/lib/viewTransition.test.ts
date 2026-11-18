import { afterEach, describe, expect, it, vi } from 'vitest';
import { canViewTransition, inViewTransition, withViewTransition } from './viewTransition';

const media = (reduce: boolean) => ({ matchMedia: () => ({ matches: reduce }) as unknown as MediaQueryList });

describe('view transitions', () => {
  afterEach(() => {
    delete (document as { startViewTransition?: unknown }).startViewTransition;
    delete document.documentElement.dataset.viewTransition;
  });

  it('are used only where the webview has them, and never with reduced motion', () => {
    expect(canViewTransition(document, media(false))).toBe(false);
    const doc = { startViewTransition: () => ({}) } as unknown as Document;
    expect(canViewTransition(doc, media(false))).toBe(true);
    expect(canViewTransition(doc, media(true))).toBe(false);
  });

  it('without them, the update simply runs', () => {
    const update = vi.fn();
    withViewTransition(update);
    expect(update).toHaveBeenCalledOnce();
    expect(inViewTransition()).toBe(false);
  });

  it('with them, the update runs inside the transition, which the new screen can tell', async () => {
    window.matchMedia = (() => ({ matches: false })) as unknown as typeof window.matchMedia;
    let seen = false;
    let done!: () => void;
    const finished = new Promise<void>((resolve) => { done = resolve; });
    (document as { startViewTransition?: unknown }).startViewTransition = (callback: () => Promise<void>) => {
      void callback().then(done);
      return { finished };
    };
    withViewTransition(() => { seen = inViewTransition(); });
    await finished;
    await Promise.resolve();
    expect(seen).toBe(true);
    expect(inViewTransition()).toBe(false);
  });
});
