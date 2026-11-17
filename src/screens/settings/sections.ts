import {
  BookA,
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
