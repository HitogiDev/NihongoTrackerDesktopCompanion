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
  MangaLibrary,
  MangaSession,
  MangaSeriesDetail,
  MangaSource,
  MokuroPage,
  EditedBlock,
  OcrExport,
  OcrStatus,
  ReaderPrefs,
  ReaderTarget,
  ReaderVolume,
  ScanSummary,
  WebdavAuth,
  WebdavCacheUsage,
  WebdavProbe,
  DownloadProgress,
  MinedSentence,
  MpvSetup,
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
  TodaySummary,
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
  mpvSetupPlan: (player: string) => invoke<MpvSetup>('mpv_setup_plan', { player }),
  mpvSetupApply: (player: string) => invoke<MpvSetup>('mpv_setup_apply', { player }),
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
  /** Today's totals from this computer's outbox, in the account's day. */
  todaySummary: () => invoke<TodaySummary>('today_summary'),
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

  // manga library
  mangaSources: () => invoke<MangaSource[]>('manga_sources'),
  /** Rejects with `not-a-folder` or `already-added` (see `mangaErrorKey`). */
  mangaAddLocalRoot: (path: string) => invoke<ScanSummary>('manga_add_local_root', { path }),
  mangaRemoveSource: (id: string) => invoke<void>('manga_remove_source', { id }),
  /** `force` for the user's button; without it a scan from the last minute is enough. */
  mangaRescan: (force = false) => invoke<ScanSummary>('manga_rescan', { force }),
  mangaLibrary: () => invoke<MangaLibrary>('manga_library'),
  /**
   * Lists a WebDAV server and counts its series. `auth` null keeps the
   * credentials `sourceId` already has (the form never receives them).
   * Rejects with a code: `offline`, `unauthorized`, `not-webdav`, `invalid-url`…
   */
  mangaWebdavTest: (url: string, auth: WebdavAuth | null, sourceId?: string) =>
    invoke<WebdavProbe>('manga_webdav_test', { url, auth, sourceId }),
  /** Adds (or edits, with `sourceId`) a WebDAV source and syncs it. Resolves with its id. */
  mangaWebdavSave: (name: string, url: string, auth: WebdavAuth | null, sourceId?: string) =>
    invoke<string>('manga_webdav_save', { sourceId, name, url, auth }),
  /** `force` lists every series, not only the folders that changed. */
  mangaWebdavSync: (sourceId: string, force = false) => invoke<void>('manga_webdav_sync', { sourceId, force }),
  /**
   * Downloads a remote volume whole (progress on `events.mangaDownload`).
   * `offline` keeps it for reading without the server; otherwise it goes to
   * the cache, for a server that cannot send parts of files.
   */
  mangaDownloadVolume: (volumeId: string, offline: boolean) => invoke<void>('manga_download_volume', { volumeId, offline }),
  mangaCancelDownload: (volumeId: string) => invoke<boolean>('manga_cancel_download', { volumeId }),
  mangaDownloads: () => invoke<DownloadProgress[]>('manga_downloads'),
  mangaRemoveDownload: (volumeId: string) => invoke<void>('manga_remove_download', { volumeId }),
  mangaWebdavCacheUsage: () => invoke<WebdavCacheUsage>('manga_webdav_cache_usage'),
  /** Empties the WebDAV cache; offline downloads stay. */
  mangaWebdavClearCache: () => invoke<WebdavCacheUsage>('manga_webdav_clear_cache'),
  /** The text a volume shows: one of its layers, or the main `.mokuro` with null. */
  mangaSetTextLayer: (volumeId: string, layer: string | null) => invoke<void>('manga_set_text_layer', { volumeId, layer }),
  /** A remote series was opened: Rust fetches its `.mokuro` files in the background. */
  mangaPrepareSeries: (seriesId: string) => invoke<void>('manga_prepare_series', { seriesId }),
  mangaSeriesDetail: (seriesId: string) => invoke<MangaSeriesDetail | null>('manga_series_detail', { seriesId }),
  /** `mediaId: null` unlinks. */
  mangaLinkSeries: (seriesId: string, mediaId: string | null, mediaTitle?: string, isAdult = false) =>
    invoke<void>('manga_link_series', { seriesId, mediaId, mediaTitle, isAdult }),
  /** Opens (or focuses) the reader window on a volume. */
  mangaOpenReader: (volumeId: string, page?: number) => invoke<void>('manga_open_reader', { volumeId, page }),
  /** The reader window asks what to show once its page loads. */
  mangaReaderTarget: () => invoke<ReaderTarget | null>('manga_reader_target'),
  mangaReaderVolume: (volumeId: string) => invoke<ReaderVolume | null>('manga_reader_volume', { volumeId }),
  mangaSaveProgress: (volumeId: string, page: number, finished: boolean) =>
    invoke<void>('manga_save_progress', { volumeId, page, finished }),
  mangaSaveReaderPrefs: (seriesId: string, prefs: ReaderPrefs) =>
    invoke<ReaderPrefs>('manga_save_reader_prefs', { seriesId, prefs }),
  /** The text of every page (`null` where there is none), for the text layer. */
  mangaVolumeText: (volumeId: string) => invoke<(MokuroPage | null)[]>('manga_volume_text', { volumeId }),
  /** The reader shows this volume: Rust starts counting it (see `manga::session`). */
  mangaSessionOpen: (volumeId: string) => invoke<void>('manga_session_open', { volumeId }),
  /** The pages on screen now; none past the last page. */
  mangaSessionPages: (volumeId: string, pages: number[]) => invoke<void>('manga_session_pages', { volumeId, pages }),
  mangaSessionState: () => invoke<MangaSession | null>('manga_session_state'),
  /** Mines a text block, with the panel around it as the card's picture. */
  mangaMineBlock: (volumeId: string, page: number, block: number) =>
    invoke<MinedSentence>('manga_mine_block', { volumeId, page, block }),
  // built-in OCR (`manga::ocr`)
  mangaOcrStatus: () => invoke<OcrStatus>('manga_ocr_status'),
  /** Reads pages without text: `visible` first, `ahead` after. Rejects with `not-installed`. */
  mangaOcrReadPages: (volumeId: string, visible: number[], ahead: number[]) =>
    invoke<void>('manga_ocr_read_pages', { volumeId, visible, ahead }),
  /** Queues every page of a volume without text; resolves with how many. */
  mangaOcrProcessVolume: (volumeId: string) => invoke<number>('manga_ocr_process_volume', { volumeId }),
  mangaOcrCancelVolume: (volumeId: string) => invoke<boolean>('manga_ocr_cancel_volume', { volumeId }),
  mangaOcrQueuedPages: (volumeId: string) => invoke<number[]>('manga_ocr_queued_pages', { volumeId }),
  /** Downloads the runtime and models, only after the consent dialog. Rejects with `unsupported` or `downloading`. */
  mangaOcrInstall: () => invoke<void>('manga_ocr_install'),
  mangaOcrCancelInstall: () => invoke<boolean>('manga_ocr_cancel_install'),
  mangaOcrDelete: () => invoke<void>('manga_ocr_delete'),
  /** The text inside a box (image pixels) of a page, read before anything queued. */
  mangaOcrRegion: (volumeId: string, page: number, bbox: [number, number, number, number]) =>
    invoke<string>('manga_ocr_region', { volumeId, page, bbox }),
  /**
   * Writes a volume's text as a `.mokuro` beside it (on its WebDAV server for a
   * remote volume). Rejects with `exists` unless `overwrite`, `read-only`, `no-text`…
   */
  mangaOcrExport: (volumeId: string, overwrite: boolean) => invoke<OcrExport>('manga_ocr_export', { volumeId, overwrite }),
  /** Saves a page's blocks as corrected by hand; they win over the `.mokuro`. */
  mangaSavePageText: (volumeId: string, page: number, imageWidth: number, imageHeight: number, blocks: EditedBlock[]) =>
    invoke<MokuroPage>('manga_save_page_text', { volumeId, page, imageWidth, imageHeight, blocks }),

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
  mangaLibrary: 'manga://library-changed',
  /** A WebDAV source being listed (`WebdavSyncProgress`). */
  mangaWebdavSync: 'manga://webdav-sync',
  /** A remote volume downloading (`DownloadProgress`). */
  mangaDownload: 'manga://download',
  /** To the reader window: show another volume. */
  mangaReaderOpen: 'manga://reader-open',
  /** The reading session (`MangaSession | null`) whenever it changes. */
  mangaSession: 'manga://session',
  /** A reading session ended (`MangaSessionEnded`). */
  mangaSessionEnded: 'manga://session-ended',
  /** `OcrStatus`: the built-in OCR's engine or a volume run changed. */
  mangaOcrStatus: 'manga://ocr-status',
  /** `OcrPage`: a page got text from the OCR (or could not be read). */
  mangaOcrPage: 'manga://ocr-page',
  /** `OcrDownload`: the OCR's runtime and models coming down. */
  mangaOcrProgress: 'manga://ocr-progress',
} as const;
