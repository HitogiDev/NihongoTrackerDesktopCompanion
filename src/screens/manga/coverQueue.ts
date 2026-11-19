/**
 * The order covers load in. A grid of a thousand remote series asked for every
 * cover in view at once and they arrived as the server answered, the bottom
 * ones before the top ones. Here a card only asks once it is near the screen,
 * waits its turn with at most `limit` loading at a time, and the next turn
 * goes to the lowest position (the grid's reading order). A card that leaves
 * the screen before its turn gives its place up; one already loading
 * finishes (it lands in the cache, and aborting would not stop Rust).
 *
 * Rust keeps the same order for the fetches that go to a server
 * (`manga::protocol::FetchGate`).
 */
export interface Ticket {
  /** Gives up a place that has not started; a no-op once it has. */
  cancel: () => void;
}

interface Waiting {
  id: number;
  priority: number;
  start: (done: () => void) => void;
}

export class CoverQueue {
  private waiting: Waiting[] = [];
  private running = 0;
  private nextId = 0;

  constructor(private readonly limit: number) {}

  /** `start` runs in its turn and calls `done` once its image settles. */
  request(priority: number, start: (done: () => void) => void): Ticket {
    const entry: Waiting = { id: this.nextId++, priority, start };
    this.waiting.push(entry);
    this.pump();
    return {
      cancel: () => {
        const index = this.waiting.indexOf(entry);
        if (index >= 0) this.waiting.splice(index, 1);
      },
    };
  }

  /** How many are waiting and loading, for tests. */
  get size() {
    return { waiting: this.waiting.length, running: this.running };
  }

  private pump() {
    while (this.running < this.limit && this.waiting.length > 0) {
      // Lowest position first; among equals, whoever asked first.
      let best = 0;
      for (let index = 1; index < this.waiting.length; index += 1) {
        const candidate = this.waiting[index];
        const current = this.waiting[best];
        if (candidate.priority < current.priority || (candidate.priority === current.priority && candidate.id < current.id)) best = index;
      }
      const [entry] = this.waiting.splice(best, 1);
      this.running += 1;
      let settled = false;
      const done = () => {
        if (settled) return;
        settled = true;
        this.running -= 1;
        this.pump();
      };
      try {
        entry.start(done);
      } catch {
        done();
      }
    }
  }
}

/** Covers of the library grid: a few at a time, like Rust's own gate. */
export const coverQueue = new CoverQueue(6);

/** Images seen this session: shown at once, no queue (a view transition back
 *  to the grid needs the picture there on the first frame). */
const loaded = new Set<string>();
export const coverSeen = (src: string) => loaded.has(src);
export const markCoverSeen = (src: string) => { loaded.add(src); };

/**
 * One observer for every queued cover: whether each is near the screen.
 * `null` without IntersectionObserver (then every cover just loads).
 */
type Watcher = (near: boolean) => void;
let observer: IntersectionObserver | null | undefined;
const watchers = new WeakMap<Element, Watcher>();

export function watchNearScreen(element: Element, onChange: Watcher): (() => void) | null {
  if (observer === undefined) {
    observer = typeof IntersectionObserver === 'undefined'
      ? null
      : new IntersectionObserver((entries) => {
        for (const entry of entries) watchers.get(entry.target)?.(entry.isIntersecting);
      }, { rootMargin: '240px 0px' });
  }
  if (!observer) return null;
  const current = observer;
  watchers.set(element, onChange);
  current.observe(element);
  return () => {
    current.unobserve(element);
    watchers.delete(element);
  };
}
