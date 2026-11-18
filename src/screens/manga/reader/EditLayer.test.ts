import { describe, expect, it } from 'vitest';
import { toImage, toScreen } from './EditLayer';

describe('boxes drawn by hand', () => {
  it('go from the page as drawn to image pixels, whichever way they were dragged', () => {
    expect(toImage({ x1: 200, y1: 150, x2: 100, y2: 50 }, 0.5)).toEqual([200, 100, 400, 300]);
    expect(toScreen([200, 100, 400, 300], 0.5)).toEqual({ left: 100, top: 50, width: 100, height: 100 });
  });

  it('never divide by a zero scale', () => {
    expect(toImage({ x1: 0, y1: 0, x2: 10, y2: 20 }, 0)).toEqual([0, 0, 10, 20]);
  });
});
