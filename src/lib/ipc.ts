import { invoke } from '@tauri-apps/api/core';
import type {
  BlacklistEntry,
  Capabilities,
  UpdateInfo,
  ConnectionStatus,
  ActivityFeedPage,
  FriendImmersionPresence,
  DiscordPreview,
  DictionaryLookupResponse,
  DictionarySummary,
  HookerState,
  HookerContext,
  HookerTotals,
  LogPayload,
  MediaHit,
  MediaMatch,
  MinedSentence,
  OutboxItem,
  PendingItem,
  ProcessEntry,
  RankingRow,
  RecentLog,
  Settings,
  NativeHookTextFilters,
  FilterPreview,
  SessionIntelligence,
  StreakStatus,
  TextHistoryEntry,
  TextLine,
  TextSessionSummary,
  TrackerSnapshot,
  UserSummary,
  VnApp,
  ReadingOverlayState,
} from './types';

// Every Rust command in one place, typed. Argument names must match the Rust
// signatures (Tauri camelCases snake_case parameters).

export const ipc = {
  // settings & auth
  getSettings: () => invoke<Settings>('get_settings'),
  previewNativeHookFilters: (filters: NativeHookTextFilters, text: string, reduceRepetitions = false, filterChinese = false) =>
    invoke<FilterPreview>('preview_native_hook_filters', { filters, text, reduceRepetitions, filterChinese }),
  saveSettings: (settings: Settings) => invoke<Settings>('save_settings', { settings }),
  openAuxWindow: (kind: 'settings' | 'texthooker' | 'reading-overlay-settings') => invoke<void>('open_aux_window', { kind }),
  openWorkTexthooker: (contentId?: string, allowStartingSession = false) =>
    invoke<void>('open_work_texthooker', { contentId, allowStartingSession }),
  verifyApiKey: (baseUrl: string, apiKey: string) =>
    invoke<UserSummary>('verify_api_key', {
      baseUrl,
      apiKey,
    }),
  /** `cached` accepts a status the background loop got in the last few seconds. */
  connectionStatus: (cached = false) => invoke<ConnectionStatus>('connection_status', { cached }),

  // tracker
  getTrackerState: () => invoke<TrackerSnapshot>('get_tracker_state'),
  listProcesses: () => invoke<ProcessEntry[]>('list_processes'),
  platformSupported: () => invoke<boolean>('platform_supported'),
  platformCapabilities: () => invoke<Capabilities>('platform_capabilities'),
  pendingUpdate: () => invoke<UpdateInfo | null>('pending_update'),
  checkForUpdate: () => invoke<UpdateInfo | null>('check_for_update'),
  /** Downloads, installs and restarts; resolves only if it fails to restart. */
  installUpdate: () => invoke<void>('install_update'),
  /** macOS: the Screen Recording prompt. Resolves to whether titles are readable now. */
  requestWindowTitlesPermission: () => invoke<boolean>('request_window_titles_permission'),
  /** Log the episode currently on screen without waiting for the threshold. */
  logCurrentAnime: (comment: string) => invoke<{ pending: boolean }>('log_current_anime', { comment }),
  parseReleaseName: (name: string) => invoke('parse_release_name', { name }),

  // pending confirmations
  listPending: () => invoke<PendingItem[]>('list_pending'),
  confirmPending: (id: string, payload?: LogPayload) =>
    invoke<{ outboxId: string }>('confirm_pending', { id, payload }),
  discardPending: (id: string) => invoke<void>('discard_pending', { id }),

  // outbox
  listOutbox: () => invoke<OutboxItem[]>('list_outbox'),
  retryOutbox: (id: string) => invoke<void>('retry_outbox', { id }),
  /** A full copy of the local database; it includes the API key. */
  exportDatabase: (path: string) => invoke<void>('export_database', { path }),
  /** Replaces the local database and restarts; resolves only on failure. */
  restoreDatabase: (path: string) => invoke<void>('restore_database', { path }),
  /** Replace a queued or failed log and send it again. */
  updateOutbox: (id: string, payload: LogPayload) => invoke<void>('update_outbox', { id, payload }),
  deleteOutbox: (id: string) => invoke<void>('delete_outbox', { id }),
  flushOutbox: () => invoke<void>('flush_outbox'),

  // logs
  createLog: (payload: LogPayload) => invoke<{ outboxId: string }>('create_log', { payload }),
  previewXp: (payload: LogPayload) =>
    invoke<{ xp: number; xpBreakdown?: unknown }>('preview_xp', { payload }),
  recentLogs: (limit = 10) => invoke<RecentLog[]>('recent_logs', { limit }),
  userStats: () => invoke<Record<string, unknown>>('user_stats'),
  /** Shares the reminder loop's cache, so calling it often costs no requests. */
  streakStatus: () => invoke<StreakStatus | null>('streak_status'),
  ranking: (args: {
    page?: number;
    limit?: number;
    filter?: string;
    timeFilter?: string;
    audience?: 'all' | 'following' | 'mutual';
    mediumType?: string;
    mediumMetric?: string;
  }) =>
    invoke<RankingRow[]>('ranking', {
      ...args,
      // The server buckets by day in this timezone.
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    }),
  activityFeed: (args: {
    scope?: 'following' | 'clubs' | 'global';
    before?: string;
    limit?: number;
  } = {}) => invoke<ActivityFeedPage>('activity_feed', args),
  friendsImmersionPresence: () => invoke<FriendImmersionPresence[]>('friends_immersion_presence'),

  // Discord Rich Presence
  /** The very same decision the watcher publishes, for the settings preview. */
  discordPreview: () => invoke<DiscordPreview>('discord_preview'),
  discordBlacklist: () => invoke<BlacklistEntry[]>('discord_blacklist'),
  addDiscordBlacklist: (entry: BlacklistEntry) =>
    invoke<BlacklistEntry[]>('add_discord_blacklist', { entry }),
  removeDiscordBlacklist: (key: string) =>
    invoke<BlacklistEntry[]>('remove_discord_blacklist', { key }),

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
  selectNativeHook: (exe: string, hookCode: string) =>
    invoke<void>('select_native_hook', { exe, hookCode }),
  launchProgram: (path: string) => invoke<void>('launch_program', { path }),

  // texthooker
  hookerState: () => invoke<HookerState>('hooker_state'),
  hookerConnect: (url?: string) => invoke<void>('hooker_connect', { url }),
  hookerDisconnect: () => invoke<void>('hooker_disconnect'),
  setHookerInputSource: (contentId: string, source: 'lunahook' | 'websocket') =>
    invoke<TrackerSnapshot['game_hook']>('set_hooker_input_source', { contentId, source }),
  setHookerSession: (contentId: string, title?: string, mediaType?: string) =>
    invoke<HookerState>('set_hooker_session', { contentId, title, mediaType }),
  hookerLines: (contentId: string, limit?: number) =>
    invoke<TextLine[]>('hooker_lines', { contentId, limit }),
  hookerLatestLine: (contentId: string) => invoke<TextLine | null>('hooker_latest_line', { contentId }),
  readingOverlayState: () => invoke<ReadingOverlayState>('reading_overlay_state'),
  readingOverlayAction: (action: 'ready' | 'toggle' | 'show' | 'hide' | 'interaction' | 'close-popup' | 'settings', popup = false) =>
    invoke<void>('reading_overlay_action', { action, popup }),
  readingOverlayPopup: (mode: 'dictionary' | 'hooks', lineId: string | null, offset: number, x: number, y: number, vertical = false, anchor?: { top: number; bottom: number }) =>
    invoke<void>('reading_overlay_popup', { mode, lineId, offset, x, y, vertical, anchor }),
  readingOverlaySelectHook: (pid: number, contentId: string, startedAt: string, hookCode: string) =>
    invoke<void>('reading_overlay_select_hook', { pid, contentId, startedAt, hookCode }),
  overlayOpenMain: (destination: string) => invoke<void>('overlay_open_main', { destination }),
  takeMainIntent: () => invoke<string | null>('take_main_intent'),
  setSessionPaused: (paused: boolean) => invoke<void>('set_session_paused', { paused }),
  mineLine: (lineId: string) => invoke<MinedSentence>('mine_line', { lineId }),
  mineLatestLine: () => invoke<MinedSentence>('mine_latest_line'),
  minedSentences: () => invoke<MinedSentence[]>('mined_sentences'),
  deleteMinedSentence: (id: string) => invoke<void>('delete_mined_sentence', { id }),
  minedScreenshot: (id: string) => invoke<string | null>('mined_screenshot', { id }),
  exportMinedToAnki: (id: string) => invoke<number>('export_mined_to_anki', { id }),
  ankiModels: () => invoke<string[]>('anki_models'),
  ankiModelFields: (model: string) => invoke<string[]>('anki_model_fields', { model }),
  mineDictionaryEntry: (word: string, reading: string, meaning: string, sentence: string, context?: { lineId: string; contentId: string }) =>
    invoke<number>('mine_dictionary_entry', { word, reading, meaning, sentence, lineId: context?.lineId, contentId: context?.contentId }),
  hookerTotals: (contentId: string) => invoke<HookerTotals>('hooker_totals', { contentId }),
  hookerContext: (contentId: string) => invoke<HookerContext>('hooker_context', { contentId }),
  refreshHookerContext: (contentId: string) => invoke<void>('refresh_hooker_context', { contentId }),
  hookerIngest: (text: string) => invoke<void>('hooker_ingest', { text }),
  hookerSetTimerActive: (active: boolean) =>
    invoke<void>('hooker_set_timer_active', { active }),
  hookerAttachMedia: (
    contentId: string,
    title: string,
    mediaType: string
  ) =>
    invoke<HookerState>('hooker_attach_media', {
      contentId,
      title,
      mediaType,
    }),
  hookerDeleteLine: (id: string) => invoke<void>('hooker_delete_line', { id }),
  hookerRestoreLine: (line: TextLine) => invoke<void>('hooker_restore_line', { line }),
  hookerHistory: (contentId: string) =>
    invoke<TextHistoryEntry[]>('hooker_history', { contentId }),
  hookerClear: (contentId: string) => invoke<void>('hooker_clear', { contentId }),
  hookerSetTimer: (contentId: string, seconds: number, updateDetectedSession = false) =>
    invoke<void>('hooker_set_timer', { contentId, seconds, updateDetectedSession }),
  listTextSessions: () => invoke<TextSessionSummary[]>('list_text_sessions'),
  logTextSession: (args: {
    content_id: string;
    media_type: string;
    description: string;
    comment: string;
    chars: number;
    seconds: number;
    intelligence: SessionIntelligence;
  }) => invoke<{ outboxId: string }>('log_text_session', { args }),
  remoteTextSession: (contentId: string) =>
    invoke<Record<string, unknown>>('remote_text_session', { contentId }),

  // offline Yomitan dictionaries
  dictionaryList: () => invoke<DictionarySummary[]>('dictionary_list'),
  dictionaryImport: (path: string) =>
    invoke<DictionarySummary>('dictionary_import', { path }),
  dictionaryRemove: (id: string) => invoke<void>('dictionary_remove', { id }),
  dictionaryLookup: (text: string, offset = 0, maxResults = 12) =>
    invoke<DictionaryLookupResponse>('dictionary_lookup', {
      text,
      offset,
      maxResults,
    }),
};

/** Event channel names emitted from Rust. */
export const events = {
  readingOverlay: 'reading-overlay://state',
  tracker: 'tracker://state',
  vnStarted: 'vn://started',
  vnEnded: 'vn://ended',
  hookerLine: 'texthooker://line',
  hookerContext: 'texthooker://context',
  mined: 'mining://changed',
  overlayNavigate: 'overlay://navigate',
  hookerStatus: 'texthooker://status',
  hookerTimer: 'texthooker://timer',
  outbox: 'outbox://changed',
  pending: 'pending://changed',
  settings: 'settings://changed',
  connection: 'connection://changed',
  celebration: 'log://celebration',
  streakReminder: 'streak://reminder',
  /** The notification (or the tray) asking for the log form. */
  streakOpenLog: 'streak://open-log',
  updateAvailable: 'updater://available',
  updateProgress: 'updater://progress',
  streakAuthRequired: 'streak://auth-required',
  discordStatus: 'discord://status',
} as const;
