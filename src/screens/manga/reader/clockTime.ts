/** The most a session's time can be edited to (Rust's `MAX_EDITED_SECS`). */
export const MAX_EDITED_SECS = 24 * 60 * 60;

/**
 * A session time typed by hand, in seconds: `h:mm:ss`, `mm:ss`, or a bare
 * number, which is minutes (what the log counts). `null` when it is not a
 * time, or past a day. Spaces are ignored; seconds and minutes after the
 * first field must be under 60.
 */
export function parseClockInput(text: string): number | null {
  const value = text.replace(/\s+/g, '');
  if (value === '') return null;
  if (/^\d+$/.test(value)) {
    const seconds = Number(value) * 60;
    return seconds <= MAX_EDITED_SECS ? seconds : null;
  }
  const parts = value.split(':');
  if (parts.length < 2 || parts.length > 3 || parts.some((part) => !/^\d+$/.test(part))) return null;
  const numbers = parts.map(Number);
  const [first, ...rest] = numbers;
  if (rest.some((part) => part >= 60)) return null;
  const seconds = numbers.length === 3
    ? first * 3600 + rest[0] * 60 + rest[1]
    : first * 60 + rest[0];
  return seconds <= MAX_EDITED_SECS ? seconds : null;
}
