/**
 * Page turns of the paged reader, as pure math: which edge a turn takes the
 * page by, how far a swipe has to go, and what a curl or a slide looks like
 * at a given point of the turn (`progress`, 0 = not started, 1 = done).
 */
import type { PageTransition, ReadingDirection } from '../../../lib/types';
import type { Size } from './layout';

/** The edge the leaving page is taken by; it moves towards the other one. */
export type TurnSide = 'left' | 'right';

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * A right-to-left book turns forward by taking the page's left edge to the
 * right (as a Japanese book does), a left-to-right one the other way; going
 * back is the mirror of going forward.
 */
export function turnSide(forward: boolean, direction: ReadingDirection): TurnSide {
  return forward === (direction === 'rtl') ? 'left' : 'right';
}

/** A swipe of `dx` px (positive = to the right) turns forward or back. */
export function swipeForward(dx: number, direction: ReadingDirection): boolean {
  return direction === 'rtl' ? dx > 0 : dx < 0;
}

/** Below this a press that moves is still a click, not a swipe. */
export const SWIPE_SLOP = 8;

/** The swipe has gone far enough to turn the page (`threshold`: % of the width; 0 = never). */
export function swipeTurns(dx: number, viewWidth: number, threshold: number): boolean {
  if (threshold <= 0) return false;
  return Math.abs(dx) >= (viewWidth * threshold) / 100;
}

/**
 * How far into the turn a swipe of `dx` px is. A slide follows the finger;
 * a curl keeps the page's free edge under it, and that edge moves twice as
 * fast as the fold.
 */
export function swipeProgress(kind: PageTransition, dx: number, viewWidth: number, sheetWidth: number): number {
  const distance = Math.abs(dx);
  const progress = kind === 'curl' ? distance / (2 * Math.max(1, sheetWidth)) : distance / Math.max(1, viewWidth);
  return Math.min(1, progress);
}

/** How long a whole turn takes, in ms. */
export const TURN_MS: Record<PageTransition, number> = { curl: 520, slide: 300, none: 0 };

export function easeOutCubic(t: number): number {
  const clamped = Math.min(1, Math.max(0, t));
  return 1 - (1 - clamped) ** 3;
}

/**
 * Where a spread's pages sit in the window, as `PagedView` lays them out:
 * side by side (in reading order, so right to left for an RTL book),
 * vertically centered on each other, the whole centered in the window, or
 * from the top (and from the reading start) when it does not fit.
 */
export function spreadBoxes(sizes: Size[], view: Size, direction: ReadingDirection): { sheet: Box; pages: Box[] } {
  const width = sizes.reduce((sum, size) => sum + size.width, 0);
  const height = sizes.reduce((most, size) => Math.max(most, size.height), 0);
  const left = width <= view.width ? (view.width - width) / 2 : direction === 'rtl' ? view.width - width : 0;
  const top = Math.max(0, (view.height - height) / 2);
  const order = sizes.map((size, index) => ({ size, index }));
  if (direction === 'rtl') order.reverse();
  const pages: Box[] = new Array(sizes.length);
  let x = left;
  for (const { size, index } of order) {
    pages[index] = { left: x, top: top + (height - size.height) / 2, width: size.width, height: size.height };
    x += size.width;
  }
  return { sheet: { left, top, width, height }, pages };
}

/** A slide: both spreads move across the window together. */
export function slideOffsets(progress: number, side: TurnSide, viewWidth: number): { leaving: number; arriving: number } {
  const sign = side === 'left' ? 1 : -1;
  return { leaving: sign * progress * viewWidth, arriving: sign * (progress - 1) * viewWidth };
}

export type Corner = 'top' | 'bottom';

/**
 * A leaf turned over its spine: `leaf` is the page that turns, `spine` the
 * edge it stays fixed by (the book's spine in a double page; the far edge
 * of the page when there is no page to land on), `corner` the corner it is
 * taken by and `landing` where that corner ends, mirrored over the spine.
 */
export interface Leaf {
  box: Box;
  spine: number;
  corner: Point;
  landing: Point;
}

export function leafOf(box: Box, side: TurnSide, corner: Corner): Leaf {
  const right = box.left + box.width;
  const x = side === 'left' ? box.left : right;
  const spine = side === 'left' ? right : box.left;
  const y = corner === 'top' ? box.top : box.top + box.height;
  return { box, spine, corner: { x, y }, landing: { x: 2 * spine - x, y } };
}

/** How high the corner lifts halfway through a turn, in page heights. */
const CORNER_ARC = 0.1;

/**
 * Where the corner is at `progress` of a turn made without a hand (a click,
 * a key, two fingers on a touchpad): from where it lies to where it lands,
 * lifted a little towards the middle of the page halfway.
 */
export function cornerPath(leaf: Leaf, progress: number): Point {
  const p = Math.min(1, Math.max(0, progress));
  const inward = leaf.corner.y === leaf.box.top ? 1 : -1;
  return {
    x: leaf.corner.x + (leaf.landing.x - leaf.corner.x) * p,
    y: leaf.corner.y + inward * CORNER_ARC * leaf.box.height * Math.sin(Math.PI * p),
  };
}

/**
 * A page fixed by its spine cannot be pulled further than it is wide: the
 * corner stays within a page width of the spine on its own side, and within
 * the page's diagonal of the spine's other end.
 */
export function constrainCorner(leaf: Leaf, point: Point): Point {
  const { box, spine, corner } = leaf;
  const near = { x: spine, y: corner.y };
  const far = { x: spine, y: corner.y === box.top ? box.top + box.height : box.top };
  const within = (from: Point, at: Point, radius: number) => {
    const dx = at.x - from.x;
    const dy = at.y - from.y;
    const distance = Math.hypot(dx, dy);
    return distance <= radius ? at : { x: from.x + (dx / distance) * radius, y: from.y + (dy / distance) * radius };
  };
  let at = within(near, point, box.width);
  at = within(far, at, Math.hypot(box.width, box.height));
  return within(near, at, box.width);
}

/** The part of a convex polygon on the side of a line where `(p - at)·normal ≥ 0`. */
export function clipPolygon(points: Point[], at: Point, normal: Point): Point[] {
  const side = (point: Point) => (point.x - at.x) * normal.x + (point.y - at.y) * normal.y;
  const out: Point[] = [];
  points.forEach((point, index) => {
    const next = points[(index + 1) % points.length];
    const a = side(point);
    const b = side(next);
    if (a >= 0) out.push(point);
    if ((a >= 0) !== (b >= 0)) {
      const t = a / (a - b);
      out.push({ x: point.x + (next.x - point.x) * t, y: point.y + (next.y - point.y) * t });
    }
  });
  return out;
}

export function boxCorners(box: Box): Point[] {
  const right = box.left + box.width;
  const bottom = box.top + box.height;
  return [{ x: box.left, y: box.top }, { x: right, y: box.top }, { x: right, y: bottom }, { x: box.left, y: bottom }];
}

export interface Fold {
  /** A point on the fold (halfway from the corner to where it is now). */
  at: Point;
  /** Across the fold, from the corner's side to where the corner went. */
  normal: Point;
  /** What is still flat of the leaf. */
  flat: Point[];
  /** What is lifted: the page under it shows there. */
  lifted: Point[];
  /** Where the lifted part lies, folded over: the leaf's back. */
  flap: Point[];
  /** The fold as a CSS `matrix()`: where each point of the leaf goes. */
  reflect: [number, number, number, number, number, number];
}

/**
 * The leaf folded so that its corner is at `point`: the fold is the line
 * halfway between the two, square to them, as it is with paper. `null`
 * while the corner has not moved.
 */
export function foldAt(leaf: Leaf, point: Point): Fold | null {
  const dx = point.x - leaf.corner.x;
  const dy = point.y - leaf.corner.y;
  const length = Math.hypot(dx, dy);
  if (length < 0.5) return null;
  const normal = { x: dx / length, y: dy / length };
  const at = { x: (leaf.corner.x + point.x) / 2, y: (leaf.corner.y + point.y) / 2 };
  const corners = boxCorners(leaf.box);
  const flat = clipPolygon(corners, at, normal);
  const lifted = clipPolygon(corners, at, { x: -normal.x, y: -normal.y });
  // Reflection over the fold: p' = p - 2((p - at)·n)n.
  const a = 1 - 2 * normal.x * normal.x;
  const b = -2 * normal.x * normal.y;
  const d = 1 - 2 * normal.y * normal.y;
  const e = at.x - a * at.x - b * at.y;
  const f = at.y - b * at.x - d * at.y;
  const reflect: Fold['reflect'] = [a, b, b, d, e, f];
  const flap = lifted.map((p) => apply(reflect, p));
  return { at, normal, flat, lifted, flap, reflect };
}

export function apply([a, b, c, d, e, f]: Fold['reflect'], point: Point): Point {
  return { x: a * point.x + c * point.y + e, y: b * point.x + d * point.y + f };
}

/**
 * The back of the leaf is the page that lands on the other side of the
 * spine: drawn where it will lie, it is carried to the folded flap by
 * mirroring it over the spine and then over the fold. Returns that
 * transform, and what of it to show (in its own, unmoved, coordinates).
 */
export function backOf(leaf: Leaf, fold: Fold): { transform: Fold['reflect']; clip: Point[] } {
  const s = leaf.spine;
  const [a, b, c, d, e, f] = fold.reflect;
  // fold ∘ mirror, the mirror being x → 2s − x.
  const transform: Fold['reflect'] = [-a, -b, c, d, a * 2 * s + e, b * 2 * s + f];
  const clip = fold.lifted.map((p) => ({ x: 2 * s - p.x, y: p.y }));
  return { transform, clip };
}

export function cssMatrix([a, b, c, d, e, f]: Fold['reflect']): string {
  return `matrix(${a}, ${b}, ${c}, ${d}, ${e}, ${f})`;
}

export function polygon(points: Point[]): string {
  // Nothing left of it: hide all (an empty polygon() would clip nothing).
  if (points.length < 3) return 'polygon(0px 0px, 0px 0px, 0px 0px)';
  return `polygon(${points.map((point) => `${round(point.x)}px ${round(point.y)}px`).join(', ')})`;
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

/**
 * A `linear-gradient` over a box of `width` × `height` running along
 * `normal` from `start`, with its stops given in px from there: CSS measures
 * stops along a line through the box's center, so they are moved onto it.
 */
export function gradientAlong(width: number, height: number, start: Point, normal: Point, stops: [string, number][]): string {
  const angle = Math.atan2(normal.x, -normal.y);
  const length = Math.abs(width * Math.sin(angle)) + Math.abs(height * Math.cos(angle));
  const origin = { x: width / 2 - (normal.x * length) / 2, y: height / 2 - (normal.y * length) / 2 };
  const offset = (start.x - origin.x) * normal.x + (start.y - origin.y) * normal.y;
  const degrees = round((angle * 180) / Math.PI);
  return `linear-gradient(${degrees}deg, ${stops.map(([color, at]) => `${color} ${round(offset + at)}px`).join(', ')})`;
}
