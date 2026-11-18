import {
  BookA,
  BookImage,
  BookOpenText,
  DatabaseBackup,
  Layers,
  MessageCircle,
  MonitorPlay,
  Palette,
  PanelTop,
  SlidersHorizontal,
  UserRound,
  type LucideIcon,
} from 'lucide-react';

export const SECTION_IDS = [
  'account',
  'anime',
  'novels',
  'manga',
  'readingOverlay',
  'dictionaries',
  'anki',
  'discord',
  'app',
  'backup',
  'advanced',
] as const;

export type SectionId = (typeof SECTION_IDS)[number];

export const SECTION_ICONS: Record<SectionId, LucideIcon> = {
  account: UserRound,
  anime: MonitorPlay,
  novels: BookOpenText,
  manga: BookImage,
  readingOverlay: PanelTop,
  dictionaries: BookA,
  anki: Layers,
  discord: MessageCircle,
  app: Palette,
  backup: DatabaseBackup,
  advanced: SlidersHorizontal,
};

export function isSectionId(value: unknown): value is SectionId {
  return typeof value === 'string' && (SECTION_IDS as readonly string[]).includes(value);
}

/**
 * What the search box can find: the label of each row (and its hint) by i18n
 * key, under the section that shows it. Static on purpose: sections render
 * only when open, so their rows cannot be read from the page.
 */
export const SEARCH_INDEX: { section: SectionId; label: string; hint?: string }[] = [
  { section: 'account', label: 'settings:account.server' },
  { section: 'account', label: 'settings:account.newKey' },
  { section: 'account', label: 'settings:account.reconnect' },
  { section: 'account', label: 'settings:account.autoReconnect' },
  { section: 'account', label: 'settings:account.signOut' },

  { section: 'anime', label: 'settings:anime.detect', hint: 'settings:anime.detectHint' },
  { section: 'anime', label: 'settings:anime.usePosition', hint: 'settings:anime.usePositionHint' },
  { section: 'anime', label: 'settings:anime.mpvIpc', hint: 'settings:anime.mpvIpcHint' },
  { section: 'anime', label: 'settings:anime.mpcPort', hint: 'settings:anime.mpcPortHint' },
  { section: 'anime', label: 'settings:anime.autoLog', hint: 'settings:anime.autoLogHint' },
  { section: 'anime', label: 'settings:anime.logTime', hint: 'settings:anime.logTimeHint' },
  { section: 'anime', label: 'settings:anime.threshold' },
  { section: 'anime', label: 'settings:anime.episodeMinutes' },

  { section: 'novels', label: 'settings:vn.detect', hint: 'settings:vn.detectHint' },
  { section: 'novels', label: 'settings:vn.requireFocus' },
  { section: 'novels', label: 'settings:vn.openTexthooker' },
  { section: 'novels', label: 'settings:vn.confirm', hint: 'settings:vn.confirmHint' },
  { section: 'novels', label: 'settings:vn.afk', hint: 'settings:vn.afkHint' },
  { section: 'novels', label: 'settings:game.detect', hint: 'settings:game.detectHint' },
  { section: 'novels', label: 'settings:filters.title' },
  { section: 'novels', label: 'settings:filters.duplicates', hint: 'settings:filters.duplicatesHint' },
  { section: 'novels', label: 'settings:filters.chinese', hint: 'settings:filters.chineseHint' },
  { section: 'novels', label: 'settings:filters.regex', hint: 'settings:filters.regexHint' },
  { section: 'novels', label: 'settings:filters.words', hint: 'settings:filters.wordsHint' },

  { section: 'manga', label: 'settings:manga.confirm', hint: 'settings:manga.confirmHint' },
  { section: 'manga', label: 'settings:manga.dwell', hint: 'settings:manga.dwellHint' },
  { section: 'manga', label: 'settings:manga.idleLog', hint: 'settings:manga.idleLogHint' },
  { section: 'manga', label: 'settings:manga.cache', hint: 'settings:manga.cacheHint' },

  { section: 'readingOverlay', label: 'readingOverlay:openSettings', hint: 'readingOverlay:settingsHint' },

  { section: 'dictionaries', label: 'settings:dictionary.import', hint: 'settings:dictionary.hint' },

  { section: 'anki', label: 'settings:anki.url', hint: 'settings:anki.urlHint' },
  { section: 'anki', label: 'settings:anki.apiKey', hint: 'settings:anki.apiKeyHint' },
  { section: 'anki', label: 'settings:anki.deck' },
  { section: 'anki', label: 'settings:anki.noteType', hint: 'settings:anki.noteTypeHint' },

  { section: 'discord', label: 'discord:enable', hint: 'discord:enableHint' },
  { section: 'discord', label: 'discord:genericMode', hint: 'discord:genericModeHint' },
  { section: 'discord', label: 'discord:idlePresence', hint: 'discord:idlePresenceHint' },
  { section: 'discord', label: 'discord:profileButton', hint: 'discord:profileButtonHint' },
  { section: 'discord', label: 'discord:blacklist.title', hint: 'discord:blacklist.hint' },

  { section: 'app', label: 'settings:app.overlay', hint: 'settings:app.overlayHint' },
  { section: 'app', label: 'settings:app.language' },
  { section: 'app', label: 'settings:app.autostart' },
  { section: 'app', label: 'settings:app.startMinimized' },
  { section: 'app', label: 'settings:app.minimizeToTray', hint: 'settings:app.minimizeToTrayHint' },
  { section: 'app', label: 'settings:app.notifications' },
  { section: 'app', label: 'settings:app.streakReminder', hint: 'settings:app.streakReminderHint' },
  { section: 'app', label: 'settings:app.streakAt', hint: 'settings:app.streakAtHint' },
  { section: 'app', label: 'settings:app.theme' },
  { section: 'app', label: 'settings:social.share', hint: 'settings:social.shareHint' },

  { section: 'backup', label: 'common:update.auto', hint: 'common:update.autoHint' },
  { section: 'backup', label: 'common:update.check' },
  { section: 'backup', label: 'settings:backup.title', hint: 'settings:backup.hint' },
  { section: 'backup', label: 'settings:backup.export' },
  { section: 'backup', label: 'settings:backup.restore' },

  { section: 'advanced', label: 'settings:platform.title' },
  { section: 'advanced', label: 'settings:app.poll', hint: 'settings:app.pollHint' },
];

/** The part of a label before its first placeholder: what the page shows
 *  whatever the current value is ("Threshold: 80%…" → "Threshold:"). */
export function labelPrefix(text: string): string {
  const cut = text.indexOf('{{');
  return (cut === -1 ? text : text.slice(0, cut)).trim();
}
