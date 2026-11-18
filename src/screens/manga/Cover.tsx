import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { BookImage } from 'lucide-react';
import { MEDIA_TYPE_CLASSES } from '../../constants/mediaColors';
import { coverQueue, coverSeen, markCoverSeen, watchNearScreen, type Ticket } from './coverQueue';

export const MANGA = MEDIA_TYPE_CLASSES.manga;

/**
 * An image from `manga://` that fades in once decoded, over a skeleton while it
 * loads and a quiet placeholder if it never does (a volume on a drive that is
 * not plugged in). Fills its parent, which sets the shape.
 */
export function Cover({ src, alt = '', className = '', eager = false, queued }: {
  src: string;
  alt?: string;
  className?: string;
  /** Above the fold: skip lazy loading. */
  eager?: boolean;
  /**
   * Its position in a grid: the image is only asked for once the card is
   * near the screen, in turn, lowest position first (see `coverQueue`).
   */
  queued?: number;
}) {
  const [state, setState] = useState<'loading' | 'ready' | 'failed'>('loading');
  const box = useRef<HTMLDivElement>(null);
  // Not queued, or seen already this session: asked for at once.
  const [granted, setGranted] = useState(() => queued === undefined || coverSeen(src));
  // The queue's turn this cover holds while its image loads.
  const turn = useRef<(() => void) | null>(null);
  const release = () => {
    turn.current?.();
    turn.current = null;
  };
  useEffect(() => {
    if (granted || queued === undefined) return;
    const element = box.current;
    if (!element) return;
    let ticket: Ticket | null = null;
    const stop = watchNearScreen(element, (near) => {
      if (near && !ticket) {
        ticket = coverQueue.request(queued, (done) => {
          turn.current = done;
          setGranted(true);
        });
      } else if (!near && ticket) {
        // Scrolled away before its turn: the place goes to what is on screen.
        ticket.cancel();
        ticket = null;
      }
    });
    if (!stop) {
      setGranted(true);
      return;
    }
    return () => {
      stop();
      ticket?.cancel();
    };
  }, [granted, queued, src]);
  // Unmounted while loading: its turn goes to the next one.
  useEffect(() => release, []);
  // Already decoded (the same cover seen a moment ago): shown at once, with
  // no fade, so a view transition morphs the picture and not its skeleton.
  const image = useRef<HTMLImageElement>(null);
  const [instant, setInstant] = useState(false);
  useLayoutEffect(() => {
    const element = image.current;
    if (element?.complete && element.naturalWidth > 0) {
      setInstant(true);
      setState('ready');
    }
  }, [src]);
  return (
    <div ref={box} className={`relative h-full w-full overflow-hidden bg-base-100 ${className}`}>
      {state === 'loading' ? <div className="skeleton absolute inset-0 rounded-none bg-base-100" /> : null}
      {state === 'failed' ? (
        <div className="absolute inset-0 grid place-items-center text-base-content/25">
          <BookImage className="h-8 w-8" aria-hidden="true" />
        </div>
      ) : null}
      <img
        ref={image}
        src={granted ? src : undefined}
        alt={alt}
        // A queued cover is asked for when its turn comes, not later.
        loading={eager || queued !== undefined ? 'eager' : 'lazy'}
        decoding="async"
        draggable={false}
        onLoad={() => {
          markCoverSeen(src);
          release();
          setState('ready');
        }}
        onError={() => {
          release();
          setState('failed');
        }}
        className={state !== 'ready'
          ? 'absolute inset-0 h-full w-full object-cover opacity-0'
          : instant
          ? 'absolute inset-0 h-full w-full object-cover opacity-100'
          : 'absolute inset-0 h-full w-full object-cover opacity-100 motion-safe:transition-opacity motion-safe:duration-300'}
      />
    </div>
  );
}

/** A slim progress bar in the manga colour, for the bottom edge of a cover. */
export function CoverProgress({ percent, label }: { percent: number; label: string }) {
  if (percent <= 0) return null;
  return (
    <div className="absolute inset-x-0 bottom-0 h-1 bg-neutral/50" role="progressbar" aria-label={label} aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      <div className={`h-full ${MANGA.accentColor}`} style={{ width: `${percent}%` }} />
    </div>
  );
}
