import { describe, expect, it } from 'vitest';
import { isSheetKey, libraryShortcuts, modifierKey, readerShortcuts } from './shortcuts';

const item = (groups: ReturnType<typeof readerShortcuts>, label: string) =>
  groups.flatMap((group) => group.items).find((entry) => entry.label === label)!;

describe('the shortcuts sheet', () => {
  it('follows the reading direction: in a right-to-left book ← is the next page', () => {
    expect(item(readerShortcuts('rtl', 'single', 'Ctrl'), 'nextPage').keys[0]).toEqual(['←']);
    expect(item(readerShortcuts('ltr', 'single', 'Ctrl'), 'nextPage').keys[0]).toEqual(['→']);
    expect(item(readerShortcuts('rtl', 'double', 'Ctrl'), 'previousPage').keys[0]).toEqual(['→']);
  });

  it('lists the group of the layout on screen first', () => {
    expect(readerShortcuts('rtl', 'vertical', 'Ctrl')[0]).toMatchObject({ title: 'strip', current: true });
    expect(readerShortcuts('rtl', 'double', 'Ctrl')[0]).toMatchObject({ title: 'pages', current: true });
  });

  it('writes the modifier as the platform does', () => {
    expect(modifierKey('MacIntel')).toBe('⌘');
    expect(modifierKey('Win32')).toBe('Ctrl');
    expect(item(readerShortcuts('rtl', 'single', '⌘'), 'mineBlock').keys).toEqual([['⌘', 'M']]);
    expect(libraryShortcuts('Ctrl')[1].items[0].keys).toEqual([['Ctrl', '1…7']]);
  });

  it('opens with ?, but not while typing', () => {
    const field = document.createElement('input');
    const base = { ctrlKey: false, metaKey: false, altKey: false };
    expect(isSheetKey({ ...base, key: '?', target: document.body })).toBe(true);
    expect(isSheetKey({ ...base, key: '?', target: field })).toBe(false);
    expect(isSheetKey({ ...base, key: '?', ctrlKey: true, target: document.body })).toBe(false);
    expect(isSheetKey({ ...base, key: '/', target: document.body })).toBe(false);
  });
});
