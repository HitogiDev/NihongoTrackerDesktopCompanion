import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { EditedBlock } from '../../../lib/types';
import type { Size } from './layout';

/** Smaller than this (drawn pixels) a drag is a click, not a box. */
const MIN_DRAG_PX = 10;

type Rect = { x1: number; y1: number; x2: number; y2: number };

/** A box in the page's drawn pixels, from one in image pixels. */
export function toScreen(bbox: EditedBlock['box'], scale: number) {
  const [x1, y1, x2, y2] = bbox;
  return { left: Math.min(x1, x2) * scale, top: Math.min(y1, y2) * scale, width: Math.abs(x2 - x1) * scale, height: Math.abs(y2 - y1) * scale };
}

/** A drag on the page, in drawn pixels, as a box in image pixels. */
export function toImage(rect: Rect, scale: number): EditedBlock['box'] {
  const s = scale > 0 ? scale : 1;
  return [Math.min(rect.x1, rect.x2) / s, Math.min(rect.y1, rect.y2) / s, Math.max(rect.x1, rect.x2) / s, Math.max(rect.y1, rect.y2) / s];
}

/**
 * Correcting a page's text by hand: every block is outlined and numbered in
 * reading order; clicking one selects it for the panel (`BlockEditor`), and
 * dragging over the page draws a new box.
 */
export function EditLayer({ pageIndex, blocks, size, imageWidth, selected, onSelect, onDraw }: {
  pageIndex: number;
  blocks: EditedBlock[];
  size: Size;
  /** The image's own width, to scale boxes to the page as drawn. */
  imageWidth: number;
  selected: number | null;
  onSelect: (page: number, block: number | null) => void;
  onDraw: (page: number, bbox: EditedBlock['box']) => void;
}) {
  const { t } = useTranslation('mangaReader');
  const scale = imageWidth > 0 ? size.width / imageWidth : 1;
  const surface = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Rect | null>(null);

  // In the page's drawn pixels (`size`), not the screen's: a zoomed page
  // (`useZoomPan`) is bigger on screen than it is drawn.
  const point = (event: ReactPointerEvent) => {
    const bounds = surface.current!.getBoundingClientRect();
    const kx = bounds.width > 0 ? size.width / bounds.width : 1;
    const ky = bounds.height > 0 ? size.height / bounds.height : 1;
    return {
      x: Math.min(Math.max(0, (event.clientX - bounds.left) * kx), size.width),
      y: Math.min(Math.max(0, (event.clientY - bounds.top) * ky), size.height),
    };
  };

  return (
    <div
      ref={surface}
      data-edit-layer
      className="manga-edit-layer absolute inset-0 cursor-crosshair"
      aria-label={t('edit.layer', { page: pageIndex + 1 })}
      onPointerDown={(event) => {
        // Ctrl+drag pans a zoomed page (`useZoomPan`): it draws nothing.
        if (event.button !== 0 || event.ctrlKey || (event.target as HTMLElement).closest('[data-edit-block]')) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        const { x, y } = point(event);
        setDrag({ x1: x, y1: y, x2: x, y2: y });
      }}
      onPointerMove={(event) => {
        if (!drag) return;
        const { x, y } = point(event);
        setDrag({ ...drag, x2: x, y2: y });
      }}
      onPointerUp={() => {
        if (!drag) return;
        const big = Math.abs(drag.x2 - drag.x1) >= MIN_DRAG_PX && Math.abs(drag.y2 - drag.y1) >= MIN_DRAG_PX;
        setDrag(null);
        if (big) onDraw(pageIndex, toImage(drag, scale));
        else onSelect(pageIndex, null);
      }}
    >
      {blocks.map((block, index) => {
        const box = toScreen(block.box, scale);
        const active = selected === index;
        return (
          <button
            key={index}
            type="button"
            data-edit-block
            className="manga-edit-block"
            data-selected={active || undefined}
            data-empty={block.text.trim() ? undefined : true}
            style={box}
            aria-pressed={active}
            aria-label={t('edit.block', { index: index + 1, text: block.text || t('edit.empty') })}
            onClick={() => onSelect(pageIndex, index)}
          >
            <span className="manga-edit-index">{index + 1}</span>
          </button>
        );
      })}
      {drag ? <div className="manga-edit-draft" style={toScreen(toImage(drag, 1), 1)} /> : null}
    </div>
  );
}
