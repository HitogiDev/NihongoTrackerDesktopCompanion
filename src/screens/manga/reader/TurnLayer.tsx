import { useSyncExternalStore } from 'react';
import { mangaUrl } from '../../../lib/manga';
import type { PageFit, ReadingDirection } from '../../../lib/types';
import { fitSpread, type Size } from './layout';
import {
  backOf,
  clipPolygon,
  constrainCorner,
  cornerPath,
  cssMatrix,
  foldAt,
  gradientAlong,
  leafOf,
  polygon,
  slideOffsets,
  spreadBoxes,
  type Box,
  type Leaf,
  type Point,
} from './turn';
import type { TurnController, TurnState } from './turnController';

/** The back of a page with no page on it: white paper. */
const PAPER = '#ffffff';
/** How much of a page's print shows through its back. */
const SHOW_THROUGH = 0.2;

function PageImg({ volumeId, page, box }: { volumeId: string; page: number; box: Box }) {
  return (
    <img
      src={mangaUrl.page(volumeId, page)}
      alt=""
      draggable={false}
      className="absolute block max-w-none select-none"
      style={{ left: box.left, top: box.top, width: box.width, height: box.height }}
    />
  );
}

function Pages({ volumeId, pages, boxes }: { volumeId: string; pages: number[]; boxes: Box[] }) {
  return <>{pages.map((page, index) => (boxes[index] ? <PageImg key={page} volumeId={volumeId} page={page} box={boxes[index]} /> : null))}</>;
}

/** Where the leaf's corner is now: in the hand, on its way from where the hand let go, or on its own path. */
function cornerNow(turn: TurnState, leaf: Leaf): Point {
  const plus = (hand: Point) => ({ x: leaf.corner.x + hand.x, y: leaf.corner.y + hand.y });
  if (turn.phase === 'drag' && turn.hand) return constrainCorner(leaf, plus(turn.hand));
  if (turn.released && turn.phase !== 'drag') {
    const commit = turn.phase === 'commit';
    const from = constrainCorner(leaf, plus(turn.released.hand));
    const to = commit ? leaf.landing : leaf.corner;
    // Released, `progress` runs from where the hand let go (0) to the end (1).
    const t = Math.min(1, Math.max(0, turn.progress));
    return constrainCorner(leaf, { x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t });
  }
  return constrainCorner(leaf, cornerPath(leaf, turn.progress));
}

function clipToBox(points: Point[], box: Box): Point[] {
  let out = points;
  out = clipPolygon(out, { x: box.left, y: 0 }, { x: 1, y: 0 });
  out = clipPolygon(out, { x: box.left + box.width, y: 0 }, { x: -1, y: 0 });
  out = clipPolygon(out, { x: 0, y: box.top }, { x: 0, y: 1 });
  return clipPolygon(out, { x: 0, y: box.top + box.height }, { x: 0, y: -1 });
}

/** In left-to-right order on screen. */
function bySlot(boxes: Box[]): number[] {
  return boxes.map((_, index) => index).sort((a, b) => boxes[a].left - boxes[b].left);
}

/**
 * The page turn on screen, drawn over the reader while it lasts. Only
 * pictures, no text layer; when it ends the reader below already shows the
 * arriving spread exactly where this drew it.
 *
 * A curl turns one leaf, taken by a corner, over its spine. In a double page
 * that is the page on the side being turned, and its back is the arriving
 * page of the other side, which it ends lying on; the arriving page of its
 * own side is under it. Without a page to land on (a page alone, before or
 * after) the whole spread folds over its far edge; its back is white paper
 * with the same print showing through, faint and mirrored.
 */
export function TurnLayer({ controller, volumeId, viewport, fit, direction, sizeOf, backgroundClass }: {
  controller: TurnController;
  volumeId: string;
  viewport: Size;
  fit: PageFit;
  direction: ReadingDirection;
  sizeOf: (page: number) => Size;
  backgroundClass: string;
}) {
  const turn = useSyncExternalStore(controller.subscribe, controller.get);
  if (!turn) return null;
  const layout = (pages: number[]) => spreadBoxes(fitSpread(pages.map(sizeOf), viewport, fit), viewport, direction);
  const leaving = layout(turn.leaving);
  const arriving = layout(turn.arriving);
  const root = `pointer-events-none absolute inset-0 z-10 overflow-hidden ${backgroundClass}`;
  const { width, height } = viewport;

  if (turn.kind === 'slide') {
    const offsets = slideOffsets(turn.progress, turn.side, width);
    return (
      <div className={root} aria-hidden="true">
        <div className="absolute inset-0" style={{ transform: `translateX(${offsets.arriving}px)` }}>
          <Pages volumeId={volumeId} pages={turn.arriving} boxes={arriving.pages} />
        </div>
        <div className="absolute inset-0" style={{ transform: `translateX(${offsets.leaving}px)` }}>
          <Pages volumeId={volumeId} pages={turn.leaving} boxes={leaving.pages} />
        </div>
      </div>
    );
  }

  const book = turn.leaving.length === 2 && turn.arriving.length === 2;
  const leavingSlots = bySlot(leaving.pages);
  const arrivingSlots = bySlot(arriving.pages);
  // The side being turned is the first slot from the left when taken by the left.
  const own = turn.side === 'left' ? 0 : 1;
  const leafIndex = book ? leavingSlots[own] : -1;
  const leafBox = book ? leaving.pages[leafIndex] : leaving.sheet;
  const leaf = leafOf(leafBox, turn.side, turn.corner);
  const fold = foldAt(leaf, cornerNow(turn, leaf));

  const flatClip = fold ? polygon(fold.flat) : undefined;
  const reach = fold ? Math.hypot(fold.at.x - leaf.corner.x, fold.at.y - leaf.corner.y) : 0;
  // The shading goes as the leaf comes to lie flat, at either end: at the
  // spine it would otherwise still be there the frame before it all goes.
  const shade = fold
    ? Math.min(1, reach / 30, Math.abs(fold.at.x - leaf.spine) / (0.2 * leaf.box.width))
    : 0;
  const away = fold ? { x: -fold.normal.x, y: -fold.normal.y } : { x: 1, y: 0 };

  return (
    <div className={root} aria-hidden="true">
      {/* Under the leaf, and the page across the spine it will land on. */}
      {book ? (
        <>
          <PageImg volumeId={volumeId} page={turn.arriving[arrivingSlots[own]]} box={arriving.pages[arrivingSlots[own]]} />
          <PageImg volumeId={volumeId} page={turn.leaving[leavingSlots[1 - own]]} box={leaving.pages[leavingSlots[1 - own]]} />
        </>
      ) : (
        <Pages volumeId={volumeId} pages={turn.arriving} boxes={arriving.pages} />
      )}

      {/* The leaf, what is still flat of it. */}
      <div className="absolute inset-0" style={{ clipPath: flatClip }}>
        {book
          ? <PageImg volumeId={volumeId} page={turn.leaving[leafIndex]} box={leafBox} />
          : <Pages volumeId={volumeId} pages={turn.leaving} boxes={leaving.pages} />}
      </div>

      {fold ? (
        <>
          {/* The fold's shadow on the page it uncovers. */}
          <div
            className="absolute inset-0"
            style={{
              clipPath: polygon(fold.lifted),
              opacity: shade,
              background: gradientAlong(width, height, fold.at, away, [['rgba(0,0,0,0.45)', 0], ['rgba(0,0,0,0)', Math.min(160, 24 + reach * 0.5)]]),
            }}
          />
          {/* The leaf's back. */}
          <div className="absolute inset-0" style={{ filter: `drop-shadow(0 0 12px rgba(0, 0, 0, ${(0.35 * shade).toFixed(3)}))` }}>
            {book ? (() => {
              const back = backOf(leaf, fold);
              const backIndex = arrivingSlots[1 - own];
              return (
                <div className="absolute inset-0" style={{ transformOrigin: '0 0', transform: cssMatrix(back.transform), clipPath: polygon(back.clip) }}>
                  <PageImg volumeId={volumeId} page={turn.arriving[backIndex]} box={arriving.pages[backIndex]} />
                </div>
              );
            })() : (
              // The back of the same page: white paper with its print showing
              // through, faint and mirrored (the leaf reflected over the fold).
              <div className="absolute inset-0" style={{ clipPath: polygon(clipToBox(fold.flap, leaving.sheet)), background: PAPER }}>
                <div className="absolute inset-0" style={{ transformOrigin: '0 0', transform: cssMatrix(fold.reflect), clipPath: polygon(fold.lifted), opacity: SHOW_THROUGH }}>
                  <Pages volumeId={volumeId} pages={turn.leaving} boxes={leaving.pages} />
                </div>
              </div>
            )}
          </div>
          {/* The back bends into the fold: darker there. */}
          <div
            className="absolute inset-0"
            style={{
              clipPath: polygon(book ? fold.flap : clipToBox(fold.flap, leaving.sheet)),
              opacity: shade,
              background: gradientAlong(width, height, fold.at, fold.normal, [['rgba(0,0,0,0.28)', 0], ['rgba(0,0,0,0)', Math.min(60, 12 + reach * 0.25)], ['rgba(255,255,255,0.08)', Math.min(120, 30 + reach * 0.5)], ['rgba(255,255,255,0)', Math.min(200, 60 + reach)]]),
            }}
          />
        </>
      ) : null}
    </div>
  );
}
