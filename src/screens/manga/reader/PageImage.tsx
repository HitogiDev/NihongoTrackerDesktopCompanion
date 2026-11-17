import { useState, type ReactNode } from 'react';
import { ImageOff } from 'lucide-react';
import { Spinner } from '../../../components/ui/Spinner';
import type { Size } from './layout';

/**
 * One page at an exact size (see `fitSpread`), so whatever is drawn over it
 * (the text layer) lines up with its pixels, and the space it takes does not
 * change when the image arrives.
 */
export function PageImage({ src, alt, size, onNaturalSize, failedLabel, children, eager = true, page }: {
  src: string;
  alt: string;
  size: Size;
  onNaturalSize?: (size: Size) => void;
  failedLabel: string;
  /** Drawn over the page once it has loaded, in its coordinates. */
  children?: ReactNode;
  eager?: boolean;
  page?: number;
}) {
  const [state, setState] = useState<'loading' | 'ready' | 'failed'>('loading');
  return (
    <div className="relative shrink-0" style={{ width: size.width, height: size.height }} data-page={page}>
      {state !== 'ready' ? (
        <div className="absolute inset-0 grid place-items-center">
          {state === 'loading' ? <Spinner size="md" className="opacity-40" /> : (
            <span className="flex flex-col items-center gap-2 text-sm opacity-60">
              <ImageOff className="h-6 w-6" aria-hidden="true" />
              {failedLabel}
            </span>
          )}
        </div>
      ) : null}
      <img
        src={src}
        alt={alt}
        draggable={false}
        decoding="async"
        loading={eager ? 'eager' : 'lazy'}
        onLoad={(event) => {
          setState('ready');
          onNaturalSize?.({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight });
        }}
        onError={() => setState('failed')}
        className={state === 'ready' ? 'block h-full w-full select-none' : 'block h-full w-full select-none opacity-0'}
      />
      {state === 'ready' ? children : null}
    </div>
  );
}
