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

  detect_visual_novels: boolean;
  afk_timeout_secs: number;
  vn_require_focus: boolean;
  vn_open_texthooker: boolean;
  vn_confirm_before_log: boolean;

  texthooker_ws_url: string;
  texthooker_autoconnect: boolean;
  texthooker_prevent_duplicates: boolean;
  texthooker_font_size: number;
  texthooker_font_family: string;
  texthooker_line_spacing: number;
  texthooker_vertical: boolean;

  poll_interval_ms: number;
  autostart: boolean;
  start_minimized: boolean;
  minimize_to_tray: boolean;
  notifications: boolean;
  theme: string;
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
  logged: boolean;
  started_at: string;
}

export interface VnSession {
  exe: string;
  content_id: string;
  media_title: string;
  media_type: string;
  image: string | null;
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
