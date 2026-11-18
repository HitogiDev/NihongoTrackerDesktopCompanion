import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { ImageOff } from 'lucide-react';
import { Spinner } from '../../../components/ui/Spinner';
import type { Size } from './layout';

/**
 * One page at an exact size (see `fitSpread`), so whatever is drawn over it
 * (the text layer) lines up with its pixels, and the space it takes does not
 * change when the image arrives. The image shows once decoded (`decode()`),
 * never half painted; one already decoded (preloaded) shows at once.
 */
export function PageImage({ src, alt, size, onNaturalSize, failedLabel, children, page }: {
  src: string;
  alt: string;
  size: Size;
  onNaturalSize?: (size: Size) => void;
  failedLabel: string;
  /** Drawn over the page once it has loaded, in its coordinates. */
  children?: ReactNode;
  page?: number;
}) {
  const [state, setState] = useState<'loading' | 'ready' | 'failed'>('loading');
  const image = useRef<HTMLImageElement>(null);
  const report = useRef(onNaturalSize);
  report.current = onNaturalSize;

  const show = (element: HTMLImageElement) => {
    report.current?.({ width: element.naturalWidth, height: element.naturalHeight });
    setState('ready');
  };

  useLayoutEffect(() => {
    const element = image.current;
    if (element?.complete && element.naturalWidth > 0) show(element);
  }, [src]);

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
        ref={image}
        src={src}
        alt={alt}
        draggable={false}
        decoding="async"
        onLoad={(event) => {
          const element = event.currentTarget;
          if (state === 'ready') return;
          // Decoded off the main thread before it is shown; a webview that
          // cannot decode it ahead shows it as loaded.
          element.decode().then(() => show(element), () => show(element));
        }}
        onError={() => setState('failed')}
        className={state === 'ready' ? 'block h-full w-full select-none' : 'block h-full w-full select-none opacity-0'}
      />
      {state === 'ready' ? children : null}
    </div>
  );
}
