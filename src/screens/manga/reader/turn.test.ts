import { describe, expect, it } from 'vitest';
import { apply, backOf, constrainCorner, cornerPath, foldAt, gradientAlong, leafOf, slideOffsets, spreadBoxes, swipeForward, swipeProgress, swipeTurns, turnSide } from './turn';

describe('page turns', () => {
  it('turns a right-to-left book forward by its left edge, and back the other way', () => {
    expect(turnSide(true, 'rtl')).toBe('left');
    expect(turnSide(false, 'rtl')).toBe('right');
    expect(turnSide(true, 'ltr')).toBe('right');
    expect(turnSide(false, 'ltr')).toBe('left');
    // A swipe to the right goes forward in a Japanese book, back in a western one.
    expect(swipeForward(40, 'rtl')).toBe(true);
    expect(swipeForward(40, 'ltr')).toBe(false);
    expect(swipeForward(-40, 'ltr')).toBe(true);
  });

  it('turns once the swipe has gone the threshold, never with 0', () => {
    expect(swipeTurns(149, 1000, 15)).toBe(false);
    expect(swipeTurns(-150, 1000, 15)).toBe(true);
    expect(swipeTurns(900, 1000, 0)).toBe(false);
  });

  it('keeps a slide under the finger, and the free edge of a curl', () => {
    expect(swipeProgress('slide', 250, 1000, 600)).toBeCloseTo(0.25);
    expect(swipeProgress('curl', -300, 1000, 600)).toBeCloseTo(0.25);
    expect(swipeProgress('slide', 5000, 1000, 600)).toBe(1);
  });

  it('slides both spreads the same way, the arriving one from off screen', () => {
    expect(slideOffsets(0, 'left', 1000)).toEqual({ leaving: 0, arriving: -1000 });
    expect(slideOffsets(0.25, 'left', 1000)).toEqual({ leaving: 250, arriving: -750 });
    expect(slideOffsets(1, 'right', 1000)).toEqual({ leaving: -1000, arriving: -0 });
  });

  it('lays out a spread like the reader does', () => {
    const view = { width: 1600, height: 900 };
    const pages = [{ width: 600, height: 900 }, { width: 640, height: 880 }];
    const rtl = spreadBoxes(pages, view, 'rtl');
    expect(rtl.sheet).toEqual({ left: 180, top: 0, width: 1240, height: 900 });
    // Right to left: the first page is on the right.
    expect(rtl.pages[0].left).toBe(180 + 640);
    expect(rtl.pages[1]).toEqual({ left: 180, top: 10, width: 640, height: 880 });
    expect(spreadBoxes(pages, view, 'ltr').pages[0].left).toBe(180);
    // Wider than the window: from where reading starts.
    expect(spreadBoxes([{ width: 2000, height: 900 }], view, 'rtl').sheet.left).toBe(-400);
    expect(spreadBoxes([{ width: 2000, height: 900 }], view, 'ltr').sheet.left).toBe(0);
  });

  // The left page of a double spread, its spine on the right at x = 600.
  const leftPage = { left: 0, top: 0, width: 600, height: 900 };

  it('takes the leaf by the corner it is grabbed by, and lands it across the spine', () => {
    const bottom = leafOf(leftPage, 'left', 'bottom');
    expect(bottom.corner).toEqual({ x: 0, y: 900 });
    expect(bottom.spine).toBe(600);
    expect(bottom.landing).toEqual({ x: 1200, y: 900 });
    expect(leafOf(leftPage, 'left', 'top').corner).toEqual({ x: 0, y: 0 });
    // The right page turned the other way: its spine is its left edge.
    const right = leafOf({ left: 600, top: 0, width: 600, height: 900 }, 'right', 'top');
    expect(right.corner).toEqual({ x: 1200, y: 0 });
    expect(right.landing).toEqual({ x: 0, y: 0 });
    // Without a hand: from the corner to its landing, lifted on the way.
    expect(cornerPath(bottom, 0)).toEqual(bottom.corner);
    expect(cornerPath(bottom, 1).x).toBeCloseTo(1200);
    expect(cornerPath(bottom, 0.5).y).toBeLessThan(900);
  });

  it('folds halfway between the corner and where the hand took it', () => {
    const leaf = leafOf(leftPage, 'left', 'bottom');
    const fold = foldAt(leaf, { x: 300, y: 900 })!;
    // Pulled straight across: the fold stands at x = 150.
    expect(fold.at).toEqual({ x: 150, y: 900 });
    expect(Math.min(...fold.flat.map((p) => p.x))).toBeCloseTo(150);
    expect(Math.max(...fold.lifted.map((p) => p.x))).toBeCloseTo(150);
    // The lifted strip lies folded over the next 150 px, the corner under the hand.
    expect(Math.max(...fold.flap.map((p) => p.x))).toBeCloseTo(300);
    expect(apply(fold.reflect, leaf.corner).x).toBeCloseTo(300);
    // Pulled up and across from the bottom corner, the fold leans.
    const lean = foldAt(leaf, { x: 300, y: 700 })!;
    expect(apply(lean.reflect, leaf.corner).x).toBeCloseTo(300);
    expect(apply(lean.reflect, leaf.corner).y).toBeCloseTo(700);
    expect(foldAt(leaf, leaf.corner)).toBeNull();
  });

  it('shows the facing page on the back, where it lands at the end', () => {
    const leaf = leafOf(leftPage, 'left', 'bottom');
    // Landed: the back is the right page, exactly where it lies.
    const landed = backOf(leaf, foldAt(leaf, leaf.landing)!);
    const corner = apply(landed.transform, { x: 1200, y: 900 });
    expect(corner.x).toBeCloseTo(1200);
    expect(corner.y).toBeCloseTo(900);
    // Halfway: the back's outer corner is under the hand, the leaf's corner.
    const half = foldAt(leaf, { x: 300, y: 900 })!;
    expect(apply(backOf(leaf, half).transform, { x: 1200, y: 900 }).x).toBeCloseTo(300);
  });

  it('keeps the corner within reach of the spine', () => {
    const leaf = leafOf(leftPage, 'left', 'bottom');
    const far = constrainCorner(leaf, { x: 2000, y: 900 });
    expect(far.x).toBeCloseTo(1200);
    const high = constrainCorner(leaf, { x: 600, y: -500 });
    expect(Math.hypot(high.x - 600, high.y - 900)).toBeLessThanOrEqual(600.01);
    // Where the hand can go, it goes.
    expect(constrainCorner(leaf, { x: 300, y: 800 })).toEqual({ x: 300, y: 800 });
  });

  it('places gradient stops from a point along a direction', () => {
    // Straight to the right over a 400 px box: CSS's line is the box's width.
    expect(gradientAlong(400, 300, { x: 100, y: 0 }, { x: 1, y: 0 }, [['red', 0], ['blue', 50]]))
      .toBe('linear-gradient(90deg, red 100px, blue 150px)');
    // Upwards: measured from the bottom.
    expect(gradientAlong(400, 300, { x: 0, y: 200 }, { x: 0, y: -1 }, [['red', 0]]))
      .toBe('linear-gradient(0deg, red 100px)');
  });
});
