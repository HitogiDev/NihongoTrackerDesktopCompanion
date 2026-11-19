import { describe, expect, it } from 'vitest';
import { MAX_EDITED_SECS, parseClockInput } from './clockTime';

describe('editing the session time', () => {
  it('reads h:mm:ss and mm:ss', () => {
    expect(parseClockInput('1:02:03')).toBe(3723);
    expect(parseClockInput('0:45:00')).toBe(2700);
    expect(parseClockInput('12:30')).toBe(750);
    expect(parseClockInput(' 1 : 00 : 00 ')).toBe(3600);
  });

  it('takes a bare number as minutes', () => {
    expect(parseClockInput('25')).toBe(1500);
    expect(parseClockInput('0')).toBe(0);
  });

  it('refuses what is not a time, or more than a day', () => {
    expect(parseClockInput('')).toBeNull();
    expect(parseClockInput('abc')).toBeNull();
    expect(parseClockInput('1:60')).toBeNull();
    expect(parseClockInput('1:2:3:4')).toBeNull();
    expect(parseClockInput('-5')).toBeNull();
    expect(parseClockInput('1.5')).toBeNull();
    expect(parseClockInput('24:00:01')).toBeNull();
    expect(parseClockInput('24:00:00')).toBe(MAX_EDITED_SECS);
  });
});
