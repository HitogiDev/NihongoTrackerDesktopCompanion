import { invoke } from '@tauri-apps/api/core';
import type {
  HookerState,
  HookerTotals,
  LogPayload,
  MediaHit,
  MediaMatch,
  OutboxItem,
  PendingItem,
  ProcessEntry,
  Settings,
  TextLine,
  TextSessionSummary,
  TrackerSnapshot,
  VnApp,
} from './types';

// Every Rust command in one place, typed. Argument names must match the Rust
// signatures (Tauri camelCases snake_case parameters).

export const ipc = {
  // settings & auth
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  verifyApiKey: (baseUrl: string, apiKey: string) =>
    invoke<{ valid: boolean; user?: Record<string, unknown> }>('verify_api_key', {
      baseUrl,
      apiKey,
    }),
  connectionStatus: () =>
    invoke<{ ok: boolean; reason?: string; user?: Record<string, unknown> }>('connection_status'),

  // tracker
  getTrackerState: () => invoke<TrackerSnapshot>('get_tracker_state'),
  listProcesses: () => invoke<ProcessEntry[]>('list_processes'),
  platformSupported: () => invoke<boolean>('platform_supported'),
  parseReleaseName: (name: string) => invoke('parse_release_name', { name }),

  // pending confirmations
  listPending: () => invoke<PendingItem[]>('list_pending'),
  confirmPending: (id: string, payload?: LogPayload) =>
    invoke<{ outboxId: string }>('confirm_pending', { id, payload }),
  discardPending: (id: string) => invoke<void>('discard_pending', { id }),

  // outbox
  listOutbox: () => invoke<OutboxItem[]>('list_outbox'),
  retryOutbox: (id: string) => invoke<void>('retry_outbox', { id }),
  deleteOutbox: (id: string) => invoke<void>('delete_outbox', { id }),
  flushOutbox: () => invoke<void>('flush_outbox'),

  // logs
  createLog: (payload: LogPayload) => invoke<{ outboxId: string }>('create_log', { payload }),
  previewXp: (payload: LogPayload) =>
    invoke<{ xp: number; xpBreakdown?: unknown }>('preview_xp', { payload }),
  recentLogs: (limit = 10) => invoke<{ logs?: unknown[] } | unknown[]>('recent_logs', { limit }),
  userStats: () => invoke<Record<string, unknown>>('user_stats'),

  // media
  searchMedia: (query: string, mediaType: string) =>
    invoke<MediaHit[]>('search_media', { query, mediaType }),
  multiSearchMedia: (query: string) => invoke<MediaHit[]>('multi_search_media', { query }),
  mediaDetail: (mediaType: string, contentId: string) =>
    invoke<Record<string, unknown>>('media_detail', { mediaType, contentId }),
  setTitleMapping: (title: string, media: MediaMatch) =>
    invoke<void>('set_title_mapping', { title, media }),
  listTitleMappings: () =>
    invoke<{ title: string; media: MediaMatch }[]>('list_title_mappings'),
  deleteTitleMapping: (title: string) => invoke<void>('delete_title_mapping', { title }),

  // VN registry
  listVnApps: () => invoke<VnApp[]>('list_vn_apps'),
  upsertVnApp: (appEntry: VnApp) => invoke<VnApp[]>('upsert_vn_app', { appEntry }),
  deleteVnApp: (exe: string) => invoke<VnApp[]>('delete_vn_app', { exe }),

  // texthooker
  hookerState: () => invoke<HookerState>('hooker_state'),
  hookerConnect: (url?: string) => invoke<void>('hooker_connect', { url }),
  hookerDisconnect: () => invoke<void>('hooker_disconnect'),
  setHookerSession: (contentId: string, title?: string, mediaType?: string) =>
    invoke<HookerState>('set_hooker_session', { contentId, title, mediaType }),
  hookerLines: (contentId: string, limit?: number) =>
    invoke<TextLine[]>('hooker_lines', { contentId, limit }),
  hookerTotals: (contentId: string) => invoke<HookerTotals>('hooker_totals', { contentId }),
  hookerIngest: (text: string) => invoke<void>('hooker_ingest', { text }),
  hookerAttachMedia: (
    fromContentId: string,
    contentId: string,
    title: string,
    mediaType: string
  ) =>
    invoke<HookerState>('hooker_attach_media', {
      fromContentId,
      contentId,
      title,
      mediaType,
    }),
  hookerDeleteLine: (id: string) => invoke<void>('hooker_delete_line', { id }),
  hookerClear: (contentId: string) => invoke<void>('hooker_clear', { contentId }),
  hookerSetTimer: (contentId: string, seconds: number) =>
    invoke<void>('hooker_set_timer', { contentId, seconds }),
  listTextSessions: () => invoke<TextSessionSummary[]>('list_text_sessions'),
  logTextSession: (args: {
    content_id: string;
    media_type: string;
    description: string;
    chars: number;
    seconds: number;
  }) => invoke<{ outboxId: string }>('log_text_session', { args }),
  remoteTextSession: (contentId: string) =>
    invoke<Record<string, unknown>>('remote_text_session', { contentId }),
};

/** Event channel names emitted from Rust. */
export const events = {
  tracker: 'tracker://state',
  vnStarted: 'vn://started',
  vnEnded: 'vn://ended',
  hookerLine: 'texthooker://line',
  hookerStatus: 'texthooker://status',
  outbox: 'outbox://changed',
  pending: 'pending://changed',
  settings: 'settings://changed',
} as const;
