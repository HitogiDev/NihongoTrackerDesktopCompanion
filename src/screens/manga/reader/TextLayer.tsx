import type { MouseEvent as ReactMouseEvent } from 'react';
import type { MokuroPage, TextLayerMode } from '../../../lib/types';
import type { Size } from './layout';
import { blockBox, blockFontSize, blockText } from './textLayer';

/**
 * mokuro's text over the page: one box per block, scaled from the image's
 * pixels to the page as drawn, with its lines in the box's writing direction.
 * The text is real text, so it can be selected and copied, and looked up with
 * the same Shift/Alt hover as the texthooker. How visible it is is the user's
 * choice (`data-mode`, styled in index.css).
 */
export function TextLayer({ page, size, mode, onPointer, onLeave, onHoverBlock }: {
  page: MokuroPage | null | undefined;
  size: Size;
  mode: TextLayerMode;
  onPointer: (event: ReactMouseEvent<HTMLElement>, text: string) => void;
  onLeave: () => void;
  /** The block under the pointer, for Ctrl+C with nothing selected. */
  onHoverBlock: (block: { text: string; element: HTMLElement } | null) => void;
}) {
  if (mode === 'hidden' || !page || page.blocks.length === 0 || page.img_width <= 0) return null;
  const scale = size.width / page.img_width;
  return (
    <div className="manga-text-layer absolute inset-0" data-mode={mode} lang="ja">
      {page.blocks.map((block, index) => {
        const text = blockText(block);
        if (!text) return null;
        return (
          <div
            key={index}
            data-text-block
            className={block.vertical ? 'manga-block manga-block-vertical' : 'manga-block'}
            style={{ ...blockBox(block, scale), fontSize: blockFontSize(block, scale) }}
            onMouseMove={(event) => onPointer(event, text)}
            onMouseEnter={(event) => onHoverBlock({ text, element: event.currentTarget })}
            onMouseLeave={() => {
              onHoverBlock(null);
              onLeave();
            }}
          >
            {block.lines.map((line, lineIndex) => <span key={lineIndex} className="manga-line">{line}</span>)}
          </div>
        );
      })}
    </div>
  );
}
