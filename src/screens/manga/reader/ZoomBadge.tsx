import { useTranslation } from 'react-i18next';
import { ZoomOut } from 'lucide-react';
import { useZoomScale, type ZoomPan } from './useZoomPan';

/**
 * How far the page is zoomed, and a way back. Its own component, subscribed
 * to the zoom, so zooming only redraws this pill and never the reader.
 */
export function ZoomBadge({ zoom }: { zoom: ZoomPan }) {
  const { t } = useTranslation('mangaReader');
  const percent = Math.round(useZoomScale(zoom) * 100);
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-20 z-20 flex justify-center" data-reader-chrome>
      <button
        type="button"
        className="pointer-events-auto inline-flex h-8 items-center gap-2 rounded-full border border-base-300/70 bg-base-200/90 px-3 text-xs tabular-nums text-base-content shadow-sm backdrop-blur-sm"
        title={`${t('zoom.reset')} (Ctrl+0)`}
        onClick={zoom.reset}
      >
        <span className="font-semibold">{t('zoom.level', { percent })}</span>
        <span className="h-3.5 w-px bg-base-content/15" aria-hidden="true" />
        <ZoomOut className="h-3.5 w-3.5 opacity-70" aria-hidden="true" />
        {t('zoom.reset')}
      </button>
    </div>
  );
}
