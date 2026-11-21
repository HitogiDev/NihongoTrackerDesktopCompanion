import { easeOutCubic, TURN_MS, type Corner, type Point, type TurnSide } from './turn';

export interface TurnState {
  kind: 'curl' | 'slide';
  side: TurnSide;
  /** The corner the leaf is taken by. */
  corner: Corner;
  /** The spread being turned away, and the one under it. */
  leaving: number[];
  arriving: number[];
  /** 0 = not started, 1 = turned. */
  progress: number;
  /** Following a swipe, finishing the turn, or putting the page back. */
  phase: 'drag' | 'commit' | 'cancel';
  /** How far the hand has moved the corner (px), when a hand holds it. */
  hand?: Point | null;
  /** Where the hand let go, for the rest of the turn to start from. */
  released?: { hand: Point; progress: number } | null;
}

export interface TurnController {
  get: () => TurnState | null;
  subscribe: (listener: () => void) => () => void;
  /** A swipe in progress: the turn follows it. */
  drag: (turn: Omit<TurnState, 'phase'>) => void;
  /** The swipe let go: finish the turn, or put the page back. */
  release: (commit: boolean) => void;
  /** A turn from a click, a key or the wheel, from the start. */
  play: (turn: Omit<TurnState, 'phase' | 'progress'>) => void;
  clear: () => void;
}

/**
 * The page turn on screen, if any, outside React: a swipe moves it on every
 * pointer event, and only the layer that draws it (`TurnLayer`) listens.
 */
export function createTurnController(): TurnController {
  let state: TurnState | null = null;
  let frame: number | null = null;
  const listeners = new Set<() => void>();
  const set = (next: TurnState | null) => {
    state = next;
    listeners.forEach((listener) => listener());
  };
  const stop = () => {
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
  };
  /** `span`: what part of a whole turn is left, when `progress` cannot say. */
  const animate = (target: 0 | 1, span?: number) => {
    stop();
    const from = state;
    if (!from) return;
    const startProgress = from.progress;
    const duration = TURN_MS[from.kind] * (span ?? Math.abs(target - startProgress));
    const started = performance.now();
    const tick = (now: number) => {
      if (!state) return;
      const t = duration > 0 ? (now - started) / duration : 1;
      if (t >= 1) {
        frame = null;
        set(null);
        return;
      }
      set({ ...state, progress: startProgress + (target - startProgress) * easeOutCubic(t) });
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
  };
  return {
    get: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    drag: (turn) => {
      stop();
      set({ ...turn, phase: 'drag' });
    },
    release: (commit) => {
      if (!state || state.phase !== 'drag') return;
      const phase = commit ? 'commit' : 'cancel';
      if (state.kind === 'curl' && state.hand) {
        // A curl goes on from where the hand left the corner, wherever that
        // is: `progress` is then the way from there (0) to its end (1).
        // How far a swipe got says little about how far the corner is from
        // its end (a long one is past 1), so the rest never takes no time.
        const left = commit ? 1 - state.progress : state.progress;
        set({ ...state, phase, released: { hand: state.hand, progress: 0 }, progress: 0 });
        animate(1, Math.min(1, Math.max(0.4, left)));
        return;
      }
      set({ ...state, phase, released: null });
      animate(commit ? 1 : 0);
    },
    play: (turn) => {
      set({ ...turn, progress: 0, phase: 'commit' });
      animate(1);
    },
    clear: () => {
      stop();
      if (state) set(null);
    },
  };
}
