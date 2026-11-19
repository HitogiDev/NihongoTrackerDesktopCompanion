import type { ReaderLayout, ReadingDirection } from '../../lib/types';

/**
 * The keys of the reader and of the Manga screen, as the shortcuts sheet (`?`)
 * lists them. Only what the handlers really do (`reader/layout.ts:keyAction`,
 * `MangaReader.tsx`, `Sidebar.tsx`): if a key changes there, it changes here.
 *
 * A key is a list of keys pressed together (`['Ctrl', 'M']`); a shortcut has
 * one or more alternative keys. `mouse` entries are gestures, said in words.
 */
export interface Shortcut {
  /** i18n key under `mangaReader:shortcuts.do`. */
  label: string;
  keys: string[][];
  /** The gesture, under `mangaReader:shortcuts.mouse`, when it is not a key. */
  mouse?: string;
}

export interface ShortcutGroup {
  /** i18n key under `mangaReader:shortcuts.group`. */
  title: string;
  items: Shortcut[];
  /** The group of the layout on screen, listed first. */
  current?: boolean;
}

/** `Ctrl` or `⌘`, as the platform writes it. */
export function modifierKey(platform = typeof navigator === 'undefined' ? '' : navigator.platform): string {
  return /mac/i.test(platform) ? '⌘' : 'Ctrl';
}

/**
 * Everything the reader does from the keyboard (and the mouse), for a book
 * read in `direction`: in a right-to-left book the next page is on the left.
 */
export function readerShortcuts(direction: ReadingDirection, layout: ReaderLayout, mod = modifierKey()): ShortcutGroup[] {
  const rtl = direction === 'rtl';
  const paged: ShortcutGroup = {
    title: 'pages',
    current: layout !== 'vertical',
    items: [
      { label: 'nextPage', keys: [[rtl ? '←' : '→'], ['↓'], ['PgDn'], ['Space']] },
      { label: 'previousPage', keys: [[rtl ? '→' : '←'], ['↑'], ['PgUp'], ['Shift', 'Space']] },
      { label: 'firstLast', keys: [['Home'], ['End']] },
      { label: 'turnByClick', keys: [], mouse: rtl ? 'clickSidesRtl' : 'clickSidesLtr' },
      { label: 'turnByWheel', keys: [], mouse: 'wheel' },
      { label: 'hideControls', keys: [], mouse: 'doubleClickMiddle' },
    ],
  };
  const strip: ShortcutGroup = {
    title: 'strip',
    current: layout === 'vertical',
    items: [
      { label: 'scrollStep', keys: [['↓'], ['↑']] },
      { label: 'scrollScreen', keys: [['Space'], ['PgDn']] },
      { label: 'scrollScreenBack', keys: [['Shift', 'Space'], ['PgUp']] },
      { label: 'firstLast', keys: [['Home'], ['End']] },
      { label: 'hideControls', keys: [], mouse: 'doubleClick' },
    ],
  };
  const text: ShortcutGroup = {
    title: 'text',
    items: [
      { label: 'textLayer', keys: [['T']] },
      { label: 'lookUp', keys: [], mouse: 'hoverModifier' },
      { label: 'nextBlock', keys: [['Tab'], ['Shift', 'Tab']] },
      { label: 'blockArrows', keys: [['↓'], ['↑']] },
      { label: 'lookUpFocused', keys: [['Enter']] },
      { label: 'copyBlock', keys: [[mod, 'C']] },
      { label: 'mineBlock', keys: [[mod, 'M']] },
      { label: 'leaveBlock', keys: [['Esc']] },
    ],
  };
  const editing: ShortcutGroup = {
    title: 'editing',
    items: [
      { label: 'editText', keys: [['E']] },
      { label: 'drawBox', keys: [], mouse: 'drag' },
      { label: 'deleteBlock', keys: [['Delete'], ['Backspace']] },
      { label: 'deselectOrSave', keys: [['Esc']] },
    ],
  };
  const window: ShortcutGroup = {
    title: 'window',
    items: [
      { label: 'fullscreen', keys: [['F']] },
      { label: 'escape', keys: [['Esc']] },
      { label: 'sheet', keys: [['?']] },
    ],
  };
  const layouts = layout === 'vertical' ? [strip, paged] : [paged, strip];
  // In this order the sheet's two columns come out about as tall.
  return [...layouts, window, text, editing];
}

/** The Manga screen, and the app's own keys that also work there. */
export function libraryShortcuts(mod = modifierKey()): ShortcutGroup[] {
  return [
    {
      title: 'library',
      items: [
        { label: 'search', keys: [['/']] },
        { label: 'browse', keys: [['Tab'], ['Shift', 'Tab']] },
        { label: 'open', keys: [['Enter']] },
        { label: 'sheet', keys: [['?']] },
      ],
    },
    {
      title: 'app',
      items: [
        { label: 'sections', keys: [[mod, '1…7']] },
        { label: 'palette', keys: [[mod, 'K']] },
        { label: 'quickLog', keys: [[mod, 'L']] },
        { label: 'texthooker', keys: [[mod, 'T']] },
        { label: 'settings', keys: [[mod, ',']] },
      ],
    },
  ];
}

/** `?` opens the sheet, unless the user is typing. */
export function isSheetKey(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'target'>): boolean {
  if (event.key !== '?' || event.ctrlKey || event.metaKey || event.altKey) return false;
  return !isTyping(event.target);
}

/** Focus is in a field, where keys are text. */
export function isTyping(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  return Boolean(element?.closest?.('input, textarea, select, [contenteditable="true"]'));
}
