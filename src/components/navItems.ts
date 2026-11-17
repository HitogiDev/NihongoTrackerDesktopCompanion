import { Activity, BookmarkPlus, Inbox, Library, Trophy, UsersRound, type LucideIcon } from 'lucide-react';

export type NavItem = { to: string; labelKey: string; icon: LucideIcon; shortcut: string; end?: boolean; badge?: 'pending' };

// The main window's sections, shared by the sidebar and the command palette.
export const TRACKER_NAV: NavItem[] = [
  { to: '/', labelKey: 'activity', icon: Activity, shortcut: '1', end: true },
  { to: '/queue', labelKey: 'pending', icon: Inbox, shortcut: '2', badge: 'pending' },
  { to: '/library', labelKey: 'library', icon: Library, shortcut: '3' },
  { to: '/mining', labelKey: 'mining', icon: BookmarkPlus, shortcut: '4' },
];

export const COMMUNITY_NAV: NavItem[] = [
  { to: '/social', labelKey: 'social', icon: UsersRound, shortcut: '5' },
  { to: '/ranking', labelKey: 'ranking', icon: Trophy, shortcut: '6' },
];
