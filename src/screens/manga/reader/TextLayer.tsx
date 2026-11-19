import type { MouseEvent as ReactMouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Copy, Pickaxe } from 'lucide-react';
import type { MokuroPage, TextLayerMode } from '../../../lib/types';
import type { Size } from './layout';
import { blockBox, blockFontSize, blockText, linePlacements } from './blockGeometry';

/** A block of the page under the pointer, for Ctrl+C, Ctrl+M and lookups. */
export interface HoveredBlock {
  text: string;
  element: HTMLElement;
  page: number;
  block: number;
}

/** Room above a block for its actions, from the top of the window (where the
 *  reader's bar is); closer than this they go below it. */
const ACTIONS_ROOM_PX = 104;

/**
 * mokuro's text over the page: one box per block, scaled from the image's
 * pixels to the page as drawn, with its lines in the box's writing direction.
 * The text is real text, so it can be selected and copied, and looked up with
 * the same Shift/Alt hover as the texthooker. How visible it is is the user's
 * choice (`data-mode`, styled in index.css). Over a block, a small bar offers
 * to copy or mine it.
 */
export function TextLayer({ page, pageIndex, size, mode, lineContext, onPointer, onLeave, onHoverBlock, onCopy, onMine, mining }: {
  page: MokuroPage | null | undefined;
  pageIndex: number;
  size: Size;
  mode: TextLayerMode;
  /** The `text_lines` id of a block, so a dictionary card keeps its sentence. */
  lineContext: (block: number) => { lineId: string; contentId: string };
  onPointer: (event: ReactMouseEvent<HTMLElement>, text: string, context: { lineId: string; contentId: string }) => void;
  onLeave: () => void;
  /** The block under the pointer, for Ctrl+C with nothing selected. */
  onHoverBlock: (block: HoveredBlock | null) => void;
  onCopy: (block: HoveredBlock) => void;
  onMine: (block: HoveredBlock) => void;
  /** `page:block` being mined right now. */
  mining: string | null;
}) {
  const { t } = useTranslation('mangaReader');
  if (mode === 'hidden' || !page || page.blocks.length === 0 || page.img_width <= 0) return null;
  const scale = size.width / page.img_width;
  const total = page.blocks.filter((block) => blockText(block)).length;
  let shown = 0;
  return (
    <div className="manga-text-layer absolute inset-0" data-mode={mode} lang="ja">
      {page.blocks.map((block, index) => {
        const text = blockText(block);
        if (!text) return null;
        const box = blockBox(block, scale, page.img_width, page.img_height > 0 ? page.img_height : Infinity);
        const placed = linePlacements(block, scale, box);
        const busy = mining === `${pageIndex}:${index}`;
        const hovered = (element: HTMLElement): HoveredBlock => ({ text, element, page: pageIndex, block: index });
        const place = (element: HTMLElement) => {
          element.dataset.actions = element.getBoundingClientRect().top < ACTIONS_ROOM_PX ? 'below' : 'above';
        };
        shown += 1;
        return (
          // A stop for Tab and the arrows (MangaReader), read out as "text n
          // of N"; focused, it is the block Enter, Ctrl+C and Ctrl+M act on.
          <div
            key={index}
            data-text-block
            data-block-page={pageIndex}
            data-block={index}
            role="group"
            tabIndex={0}
            aria-label={t('text.block', { n: shown, count: total, page: pageIndex + 1 })}
            className={`manga-block ${block.vertical ? 'manga-block-vertical' : ''} ${placed ? 'manga-block-placed' : ''}`}
            style={{ ...box, fontSize: blockFontSize(block, scale) }}
            onMouseMove={(event) => onPointer(event, text, lineContext(index))}
            onMouseEnter={(event) => {
              place(event.currentTarget);
              onHoverBlock(hovered(event.currentTarget));
            }}
            onMouseLeave={() => {
              onHoverBlock(null);
              onLeave();
            }}
            onFocus={(event) => {
              if (event.target !== event.currentTarget) return;
              place(event.currentTarget);
              onHoverBlock(hovered(event.currentTarget));
            }}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onHoverBlock(null);
            }}
          >
            {/* In mokuro's order in the DOM (selection, copy, screen readers),
                each where it is on the page when the block says so. */}
            {block.lines.map((line, lineIndex) => <span key={lineIndex} className="manga-line" style={placed?.[lineIndex]}>{line}</span>)}
            <div
              className="manga-block-actions"
              data-busy={busy || undefined}
              onMouseMove={(event) => event.stopPropagation()}
              onMouseDown={(event) => event.preventDefault()}
            >
              <div className="flex items-center gap-0.5 rounded-full border border-base-300 bg-base-100 p-0.5 text-base-content shadow-lg">
                <button
                  type="button"
                  className="btn btn-ghost btn-xs btn-circle"
                  aria-label={t('text.copy')}
                  title={`${t('text.copy')} (Ctrl+C)`}
                  onClick={(event) => onCopy(hovered(event.currentTarget.closest<HTMLElement>('[data-text-block]')!))}
                >
                  <Copy className="h-3.5 w-3.5" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-xs gap-1 rounded-full px-2"
                  aria-label={t('text.mine')}
                  title={`${t('text.mine')} (Ctrl+M)`}
                  disabled={busy}
                  onClick={(event) => onMine(hovered(event.currentTarget.closest<HTMLElement>('[data-text-block]')!))}
                >
                  {busy ? <span className="loading loading-spinner loading-xs" aria-hidden="true" /> : <Pickaxe className="h-3.5 w-3.5" aria-hidden="true" />}
                  <span className="text-[11px] font-medium">{t('text.mine')}</span>
                </button>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
