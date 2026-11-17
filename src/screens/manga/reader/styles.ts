import type { ReaderBackground } from '../../../lib/types';

// Whole class names, never built: Tailwind and the CSS only see literal text.
// The colours themselves are in index.css (`.manga-bg-*`): they are the page's
// surroundings, not part of the app's theme.
export const BACKGROUND_CLASS: Record<ReaderBackground, string> = {
  black: 'manga-bg-black',
  gray: 'manga-bg-gray',
  paper: 'manga-bg-paper',
};

/** A light background needs dark controls over it, and the other way round. */
export const BACKGROUND_IS_LIGHT: Record<ReaderBackground, boolean> = { black: false, gray: false, paper: true };
