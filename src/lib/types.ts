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

export interface ReadingOverlaySettings {
  font_size: number;
  font_family: string;
  font_weight: 400 | 500 | 600 | 700;
  text_outline_width: number;
  text_outline_color: string;
  text_shadow_color: string;
  text_shadow_opacity: number;
  text_shadow_blur: number;
  center_text: boolean;
  opacity: number;
  idle_opacity: number;
  text_visibility: 'always' | 'hover_only' | 'dim_when_idle';
  always_on_top: boolean;
  hide_when_unfocused: boolean;
  toolbar_title_hover_only: boolean;
  lookup_delay_ms: number;
  popup_enabled: boolean;
  popup_background_opacity: number;
  popup_close_delay_ms: number;
  toggle_shortcut: string;
  interaction_shortcut: string;
}

export interface ReadingOverlayState {
  target: { pid: number; exe: string; content_id: string; title: string; started_at: string } | null;
  line: TextLine | null;
  visible: boolean;
  click_through: boolean;
  toolbar_hovered: boolean;
  reveal_controls: boolean;
  popup: { mode: 'dictionary' | 'hooks'; line: TextLine | null; offset: number } | null;
  shortcut_errors: string[];
}

export interface Settings {
  api_base_url: string;
  api_key: string | null;
  username: string | null;
  auto_reconnect: boolean;

  detect_players: boolean;
  anime_auto_log: boolean;
  anime_threshold_percent: number;
  anime_episode_minutes: number;
  anime_log_time: boolean;
  use_player_position: boolean;
  mpv_ipc_path: string;
  mpc_web_port: number;

  detect_visual_novels: boolean;
  detect_games: boolean;
  afk_timeout_secs: number;
  vn_require_focus: boolean;
  vn_open_texthooker: boolean;
  vn_confirm_before_log: boolean;
  overlay_enabled: boolean;
  anki_deck: string;
  anki_model: string;
  anki_field_sources: Record<string, string>;

  texthooker_ws_url: string;
  texthooker_autoconnect: boolean;
  texthooker_clipboard: boolean;
  texthooker_prevent_duplicates: boolean;
  texthooker_filter_chinese: boolean;
  native_hook_text_filters: NativeHookTextFilters;
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
  dictionary_popup_delay_ms: number;
  reading_overlay: ReadingOverlaySettings;
  texthooker_intelligence_mode: 'automatic' | 'manual' | 'off';
  texthooker_intelligence_manual_threshold_seconds: number;
  texthooker_intelligence_afk_threshold_seconds: number;
  texthooker_room_mode: string;
  texthooker_room_id: string;

  poll_interval_ms: number;
  autostart: boolean;
  start_minimized: boolean;
  minimize_to_tray: boolean;
  notifications: boolean;
  auto_update_check: boolean;
  streak_reminder: boolean;
  streak_reminder_hours: number;
  /** `HH:MM` in the user's timezone, empty to disable the fixed reminder. */
  streak_reminder_at: string;

  discord_rpc: boolean;
  discord_app_id: string;
  discord_generic_mode: boolean;
  discord_profile_button: boolean;
  discord_large_image: string;
  discord_show_idle: boolean;
  share_activity: boolean;
  theme: string;
  /** `en` | `es`, or empty to follow the system on first run. */
  language: string;
}

export interface NativeHookTextFilters {
  excluded_words: string;
  regex_patterns: string;
}

export interface FilterPreview {
  text: string;
  chars: number;
  regex_error: { line: number; message: string } | null;
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
  pid: number;
  content_id: string;
  media_title: string;
  media_type: string;
  image: string | null;
  is_adult: boolean;
  is_adult_image: boolean | null;
  elapsed_secs: number;
  counting: boolean;
  paused_by_user: boolean;
  started_at: string;
  session_chars: number;
  session_lines: number;
}

/** Mirror of `updater::UpdateInfo`. */
export interface UpdateInfo {
  version: string;
  current_version: string;
  notes: string | null;
  date: string | null;
  /** False for .deb/.rpm installs, which only the package manager can replace. */
  can_install: boolean;
}

export interface UpdateProgress {
  downloaded?: number;
  total?: number | null;
  done?: boolean;
}

/** Mirror of `platform::Capabilities`: what detection can see on this OS. */
export interface Capabilities {
  os: 'windows' | 'linux' | 'macos' | string;
  session: 'x11' | 'wayland' | null;
  window_titles: boolean;
  foreground: boolean;
  idle: boolean;
  media_sessions: boolean;
  native_hook: boolean;
  screenshots: boolean;
  actionable_notifications: boolean;
  needs_screen_recording: boolean;
}

export interface TrackerSnapshot {
  platform_supported: boolean;
  capabilities: Capabilities;
  enabled: boolean;
  active_window: string | null;
  active_process: string | null;
  idle_seconds: number;
  anime: AnimeSession | null;
  vn: VnSession | null;
  queued_count: number;
  pending_count: number;
  game_hook: NativeGameHookStatus;
}

export type NativeGameHookPhase = 'inactive' | 'starting' | 'attached' | 'receiving' | 'error' | 'unavailable';

export interface NativeGameHookStatus {
  phase: NativeGameHookPhase;
  content_id: string | null;
  exe: string | null;
  pid: number | null;
  message: string | null;
  hooks: NativeHookCandidate[];
  selected_hook_code: string | null;
  websocket_selected: boolean;
}

export interface NativeHookCandidate {
  selection_key: string;
  hook_code: string;
  name: string;
  sample: string | null;
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

export interface VnApp {
  exe: string;
  media_id: string;
  media_type: string;
  media_title: string;
  image: string | null;
  launch_path: string | null;
  hooker_path: string | null;
  native_hook_enabled: boolean;
  native_hook_code: string | null;
  reader_mode: 'integrated' | 'browser' | 'overlay';
  enabled: boolean;
  /** Work rating for games and VNs with no cached cover rating. */
  is_adult: boolean;
  is_adult_image: boolean | null;
}

export interface MinedSentence {
  id: string;
  line_id: string;
  content_id: string;
  text: string;
  media_title: string | null;
  created_at: string;
  anki_note_id: number | null;
  screenshot_path: string | null;
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
export type DiscordReason = 'full' | 'adult' | 'adultimage' | 'unresolved' | 'generic';

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

export interface DictionarySummary {
  id: string;
  title: string;
  revision: string;
  language: string;
  termCount: number;
}

export interface DictionaryEntry {
  dictionary: string;
  expression: string;
  reading: string;
  tags: string;
  score: number;
  matched: string;
  deinflected: string;
  process: string[];
  glossaries: unknown[];
  frequencies: {
    dictionary: string;
    display: string;
    value: number;
  }[];
}

export interface DictionaryLookupResponse {
  query: string;
  matched: string;
  entries: DictionaryEntry[];
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
