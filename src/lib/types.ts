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

export interface Settings {
  api_base_url: string;
  api_key: string | null;
  username: string | null;

  detect_players: boolean;
  anime_auto_log: boolean;
  anime_threshold_percent: number;
  anime_episode_minutes: number;
  anime_log_time: boolean;
  use_player_position: boolean;
  mpv_ipc_path: string;
  mpc_web_port: number;

  detect_visual_novels: boolean;
  afk_timeout_secs: number;
  vn_require_focus: boolean;
  vn_open_texthooker: boolean;
  vn_confirm_before_log: boolean;

  texthooker_ws_url: string;
  texthooker_autoconnect: boolean;
  texthooker_clipboard: boolean;
  texthooker_prevent_duplicates: boolean;
  texthooker_font_size: number;
  texthooker_font_family: string;
  texthooker_line_spacing: number;
  texthooker_line_gap: number;
  texthooker_vertical: boolean;
  texthooker_theme: string;
  texthooker_show_timer: boolean;
  texthooker_show_speed: boolean;
  texthooker_show_characters: boolean;
  texthooker_show_lines: boolean;
  texthooker_force_topbar_vertical: boolean;
  texthooker_topbar_hidden: boolean;
  texthooker_custom_css: string;
  texthooker_auto_pause_timeout: number;
  texthooker_autostart_by_line: boolean;
  texthooker_autostart_by_clipboard: boolean;
  texthooker_allow_line_during_pause: boolean;
  texthooker_allow_clipboard_during_pause: boolean;
  texthooker_pause_on_disconnect: boolean;
  texthooker_room_mode: string;
  texthooker_room_id: string;

  poll_interval_ms: number;
  autostart: boolean;
  start_minimized: boolean;
  minimize_to_tray: boolean;
  notifications: boolean;
  streak_reminder: boolean;
  streak_reminder_hours: number;
  /** `HH:MM` in the user's timezone, empty to disable the fixed reminder. */
  streak_reminder_at: string;

  discord_rpc: boolean;
  discord_app_id: string;
  discord_generic_mode: boolean;
  discord_profile_button: boolean;
  discord_large_image: string;
  theme: string;
  /** `en` | `es`, or empty to follow the system on first run. */
  language: string;
}

export interface ParsedAnime {
  title: string;
  episode: number | null;
  episode_end: number | null;
  season: number | null;
  group: string | null;
  file_name: string;
}

export interface MediaMatch {
  content_id: string;
  title: string;
  media_type: string;
  image: string | null;
  episode_duration: number | null;
  /** Never published to Discord when true. */
  is_adult: boolean;
  manual: boolean;
}

export interface AnimeSession {
  key: string;
  player: string;
  file_name: string;
  parsed: ParsedAnime;
  media: MediaMatch | null;
  /** `false` while the media lookup is still running. */
  resolved: boolean;
  unmatched: boolean;
  elapsed_secs: number;
  threshold_secs: number;
  threshold_percent: number;
  /** Reported by mpv / MPC; null when the player has no API. */
  position_secs: number | null;
  duration_secs: number | null;
  paused: boolean;
  source: 'player' | 'elapsed';
  /** 0–1 towards being logged, computed in Rust. */
  progress: number;
  logged: boolean;
  started_at: string;
}

export interface VnSession {
  exe: string;
  content_id: string;
  media_title: string;
  media_type: string;
  image: string | null;
  is_adult: boolean;
  elapsed_secs: number;
  counting: boolean;
  started_at: string;
  session_chars: number;
  session_lines: number;
}

export interface TrackerSnapshot {
  platform_supported: boolean;
  enabled: boolean;
  active_window: string | null;
  active_process: string | null;
  idle_seconds: number;
  anime: AnimeSession | null;
  vn: VnSession | null;
  queued_count: number;
  pending_count: number;
}

export type HookerStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface HookerState {
  status: HookerStatus;
  url: string;
  content_id: string | null;
  content_title: string | null;
  last_error: string | null;
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

/**
 * Whether the streak is about to be lost. Computed in Rust so the day key is
 * derived once, in the user's timezone, exactly like the backend does it.
 */
export interface StreakStatus {
  currentStreak: number;
  lastDayKey: string | null;
  todayKey: string;
  timezone: string;
  atRisk: boolean;
  minutesLeft: number;
}

/** One row of `GET /api/users/ranking`. */
export interface RankingRow {
  username: string;
  avatar?: string;
  stats: Record<string, number>;
}

/** One row of `GET /api/users/:username/logs`, which answers a bare array. */
export interface RecentLog {
  _id: string;
  type: LogType | string;
  description: string;
  episodes?: number;
  time?: number;
  chars?: number;
  pages?: number;
  xp: number;
  date: string;
  media?: {
    contentImage?: string | null;
    coverImage?: string | null;
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
    listeningLevel?: number;
    currentStreak?: number;
    longestStreak?: number;
  };
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
  synonyms?: string[] | null;
  type?: string;
  episodes?: number | null;
  episodeDuration?: number | null;
}

export interface VnApp {
  exe: string;
  media_id: string;
  media_type: string;
  media_title: string;
  image: string | null;
  texthooker_url: string | null;
  enabled: boolean;
  /** `isAdult` from the media entry, kept so the presence can strip the title. */
  is_adult: boolean;
}

/** One entry of the Discord presence blacklist. */
export interface BlacklistEntry {
  key: string;
  kind: 'media' | 'process';
  label: string;
}

/** Exactly what other people would see on Discord. */
export interface DiscordPresence {
  details: string;
  state: string;
  largeImage: string;
  largeText: string;
  startEpoch: number;
  button: { label: string; url: string } | null;
  generic: boolean;
}

/** Why the presence looks the way it does. */
export type DiscordReason = 'full' | 'adult' | 'unresolved' | 'generic';

export type DiscordDecision =
  | { status: 'off' }
  | { status: 'idle' }
  | { status: 'blocked'; label: string }
  | { status: 'show'; presence: DiscordPresence; reason: DiscordReason };

export interface DiscordPreview {
  decision: DiscordDecision;
  connected: boolean;
}

export interface ProcessEntry {
  name: string;
  pid: number;
  title: string | null;
}

/** The body sent to `POST /api/logs`, plus the local-only `_detected` block. */
export interface LogPayload {
  type: LogType | string;
  description: string;
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

export interface TextLine {
  id: string;
  content_id: string;
  text: string;
  chars: number;
  created_at: string;
  synced: boolean;
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
