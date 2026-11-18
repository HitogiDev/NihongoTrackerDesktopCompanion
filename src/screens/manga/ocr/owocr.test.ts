import { describe, expect, it } from 'vitest';
import { owocrCommand, parsePort } from './owocr';

describe('owocr settings', () => {
  it('gives the command with the port only when it is not the default', () => {
    expect(owocrCommand(7331)).toBe('owocr -r websocket -w websocket -of json');
    expect(owocrCommand(8000)).toBe('owocr -r websocket -w websocket -of json -wp 8000');
  });

  it('accepts only real ports', () => {
    expect(parsePort('7331')).toBe(7331);
    expect(parsePort(' 80 ')).toBe(80);
    expect(parsePort('0')).toBeNull();
    expect(parsePort('65536')).toBeNull();
    expect(parsePort('')).toBeNull();
    expect(parsePort('12.5')).toBeNull();
    expect(parsePort('1e3')).toBeNull();
  });
});
