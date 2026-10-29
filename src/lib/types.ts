// Types Rust sends to the UI are generated from the Rust structs by ts-rs
// (`cargo test` rewrites src/lib/bindings/). Don't copy them here: change the
// Rust type and re-run the tests. What stays in this file is what Rust passes
// through untyped (API responses) or builds with `json!`.
import type { AnimeSession } from './bindings/AnimeSession';
import type { BlacklistEntry } from './bindings/BlacklistEntry';
import type { Capabilities } from './bindings/Capabilities';
import type { DictionaryEntry } from './bindings/DictionaryEntry';
import type { DictionaryLookupResponse } from './bindings/DictionaryLookupResponse';
import type { DictionarySummary } from './bindings/DictionarySummary';
import type { DiscordDecision } from './bindings/DiscordDecision';
import type { DiscordPresence } from './bindings/DiscordPresence';
import type { DiscordReason } from './bindings/DiscordReason';
import type { FilterPreview } from './bindings/FilterPreview';
import type { HookerState } from './bindings/HookerState';
import type { HookerStatus } from './bindings/HookerStatus';
import type { MediaMatch } from './bindings/MediaMatch';
import type { MinedSentence } from './bindings/MinedSentence';
import type { NativeGameHookPhase } from './bindings/NativeGameHookPhase';
import type { NativeGameHookStatus } from './bindings/NativeGameHookStatus';
import type { NativeHookCandidate } from './bindings/NativeHookCandidate';
import type { NativeHookTextFilters } from './bindings/NativeHookTextFilters';
import type { ParsedAnime } from './bindings/ParsedAnime';
import type { ProcessEntry } from './bindings/ProcessEntry';
import type { ReadingOverlaySettings } from './bindings/ReadingOverlaySettings';
import type { ReadingOverlayState } from './bindings/ReadingOverlayState';
import type { Settings } from './bindings/Settings';
import type { StreakStatus } from './bindings/StreakStatus';
import type { TextLine } from './bindings/TextLine';
import type { TrackerSnapshot } from './bindings/TrackerSnapshot';
import type { UpdateInfo } from './bindings/UpdateInfo';
import type { VnApp } from './bindings/VnApp';
import type { VnSession } from './bindings/VnSession';

export type {
  AnimeSession,
  BlacklistEntry,
  Capabilities,
  DictionaryEntry,
  DictionaryLookupResponse,
  DictionarySummary,
  DiscordDecision,
  DiscordPresence,
  DiscordReason,
  FilterPreview,
  HookerState,
  HookerStatus,
  MediaMatch,
  MinedSentence,
  NativeGameHookPhase,
  NativeGameHookStatus,
  NativeHookCandidate,
  NativeHookTextFilters,
  ParsedAnime,
  ProcessEntry,
  ReadingOverlaySettings,
  ReadingOverlayState,
  Settings,
  StreakStatus,
  TextLine,
  TrackerSnapshot,
  UpdateInfo,
  VnApp,
  VnSession,
};

/** Log types accepted by the NihongoTracker backend (`Backend/src/models/log.model.ts`). */
export type LogType =
  | 'reading'
  | 'anime'
  | 'vn'
  | 'video'
  | 'manga'
  | 'audio'
  | 'movie'
  | 'tv show'
  | 'other'
  | 'game'
  | 'book';

export interface UpdateProgress {
  downloaded?: number;
  total?: number | null;
  done?: boolean;
}

/** `celebration` from the `POST /api/logs` response (`ILogCelebration`). */
export interface LogCelebration {
  xpGained: number;
  streak: number;
  levelUp?: { from: number; to: number };
  xp: {
    current: number;
    toCurrentLevel: number;
    toNextLevel: number;
    level: number;
  };
  rank?: {
    timeframe: string;
    rank: number;
    previousRank: number;
    overtaken: { username: string; avatar?: string; xp?: number }[];
  };
}

/** One row of `GET /api/users/ranking`. */
export interface RankingRow {
  username: string;
  avatar?: string;
  stats: Record<string, number>;
  xp?: number;
  hours?: number;
  episodes?: number;
  chars?: number;
  pages?: number;
  patreon?: { isActive?: boolean; tier?: string | null; customBadgeText?: string; badgeColor?: string; badgeTextColor?: string; hideBadge?: boolean };
  customization?: { nameEffect?: 'none' | 'gradient' | 'glow' | 'shimmer'; nameColor1?: string; nameColor2?: string; avatarFrame?: 'none' | 'bronze' | 'silver' | 'gold' | 'sakura' | 'neon' | 'rainbow' | 'aura' };
}

export interface SocialActivity {
  _id: string;
  actor: { _id?: string; username: string; avatar?: string };
  type: string;
  targetType: string;
  targetId: string;
  metadata?: Record<string, unknown>;
  occurredAt: string;
  importance?: 'normal' | 'important';
}

export interface ActivityFeedPage {
  activities: SocialActivity[];
  nextCursor: string | null;
}

export interface FriendImmersionPresence {
  username: string;
  avatar?: string;
  mediaTitle: string;
  mediaType: string;
  startedAt: string;
  elapsedSeconds: number;
  customization?: RankingRow['customization'];
  patreon?: RankingRow['patreon'];
}

/** One row of `GET /api/users/:username/logs`, which answers a bare array. */
export interface RecentLog {
  _id: string;
  type: LogType | string;
  description: string;
  mediaTitle?: string;
  episodes?: number;
  time?: number;
  chars?: number;
  pages?: number;
  xp: number;
  date: string;
  media?: {
    contentImage?: string | null;
    coverImage?: string | null;
    title?: {
      contentTitleNative?: string;
      contentTitleEnglish?: string;
      contentTitleRomaji?: string;
    };
  } | null;
}

/** Subset of the user returned by `GET /api/auth/verify`. */
export interface UserSummary {
  _id?: string;
  username?: string;
  avatar?: string;
  stats?: {
    userLevel?: number;
    userXp?: number;
    userXpToNextLevel?: number;
    userXpToCurrentLevel?: number;
    readingLevel?: number;
    readingXp?: number;
    readingXpToCurrentLevel?: number;
    readingXpToNextLevel?: number;
    listeningLevel?: number;
    listeningXp?: number;
    listeningXpToCurrentLevel?: number;
    listeningXpToNextLevel?: number;
    currentStreak?: number;
    longestStreak?: number;
  };
}

export interface ConnectionStatus {
  ok: boolean;
  reason?: string;
  user?: UserSummary;
}

export interface MediaHit {
  contentId: string;
  title: {
    contentTitleNative: string;
    contentTitleRomaji?: string;
    contentTitleEnglish?: string;
  };
  contentImage: string | null;
  coverImage: string | null;
  isAdult: boolean;
  isAdultImage?: boolean | null;
  synonyms?: string[] | null;
  type?: string;
  episodes?: number | null;
  episodeDuration?: number | null;
}

export interface DiscordPreview {
  decision: DiscordDecision;
  connected: boolean;
}

/** The body sent to `POST /api/logs`, plus the local-only `_detected` block. */
export interface LogPayload {
  type: LogType | string;
  description: string;
  mediaTitle?: string;
  mediaId?: string;
  episodes?: number;
  time?: number;
  chars?: number;
  pages?: number;
  date?: string;
  _detected?: {
    episode?: number | null;
    season?: number | null;
    file?: string;
    player?: string;
    image?: string | null;
    source?: string;
    exe?: string;
    lines?: number;
    seconds?: number;
  };
}

export interface PendingItem {
  id: string;
  createdAt: string;
  source: string;
  payload: LogPayload;
}

export interface OutboxItem {
  id: string;
  kind: string;
  status: 'queued' | 'sent' | 'failed';
  attempts: number;
  lastError: string | null;
  createdAt: string;
  payload: LogPayload | Record<string, unknown>;
}

/** Mirrors the server's `sessionHistory` rows, stored locally too. */
export interface TextHistoryEntry {
  id: string;
  contentId: string;
  loggedAt: string;
  charactersLogged: number;
  linesLogged: number;
  sessionSeconds: number;
  readingSpeed: number;
  intelligence?: SessionIntelligence;
}

export interface SessionIntelligence {
  detectionMode: 'automatic' | 'manual' | 'off';
  baselineIntervalSeconds: number | null;
  distractionThresholdSeconds: number | null;
  afkThresholdSeconds: number;
  focusedSeconds: number;
  distractedSeconds: number;
  afkSeconds: number;
  distractionCount: number;
  longestDistractionSeconds: number;
  focusPercentage: number;
  charactersPerMinute: number[];
}

export interface HookerTotals {
  lines: number;
  chars: number;
  timerSeconds: number;
  baselineChars: number;
  baselineLines: number;
  sessionChars: number;
  sessionLines: number;
  mediaTitle: string | null;
  mediaType: string | null;
}

export interface HookerContext {
  cover: string | null;
  totalCharacters: number | null;
  loggedCharacters: number;
  lines: number;
  characters: number;
  history: TextHistoryEntry[];
  updatedAt: string | null;
}

export interface TextSessionSummary {
  contentId: string;
  mediaTitle: string | null;
  mediaType: string | null;
  timerSeconds: number;
  lines: number;
  chars: number;
  sessionChars: number;
  sessionLines: number;
}
