import { useEffect, useRef, type RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { Activity, Brain, Copy, HelpCircle, Monitor, Settings, Share2, Trash2, X } from 'lucide-react';
import { Field } from '../../components/ui/Field';
import { NativeHookTextFilters } from '../../components/NativeHookTextFilters';
import { randomRoomId } from '../../lib/room';
import { useAppStore } from '../../store/useAppStore';
import { FONT_FAMILY_OPTIONS, HOOKER_THEME_OPTIONS } from './constants';
import type { RoomMode } from './useRoom';

export type SettingsTab = 'display' | 'behavior' | 'intelligence';

interface SettingsPanelProps {
  settingsButtonRef: RefObject<HTMLButtonElement | null>;
  activeSettingsTab: SettingsTab;
  setActiveSettingsTab: (tab: SettingsTab) => void;
  onClose: () => void;
  onOpenClassHelp: () => void;
  roomMode: RoomMode;
  roomId: string;
  roomConnected: boolean;
  roomError: string | null;
  inviteLink: string;
  inviteCopied: boolean;
  copyInvite: () => void;
  connectRoom: () => void;
  disconnectRoom: () => void;
  hasLines: boolean;
  clearingLines: boolean;
  onCopyAll: () => void;
  onClearAll: () => void;
}

/** The reader's settings, mounted only while open. */
export function SettingsPanel({
  settingsButtonRef, activeSettingsTab, setActiveSettingsTab, onClose, onOpenClassHelp,
  roomMode, roomId, roomConnected, roomError, inviteLink, inviteCopied, copyInvite,
  connectRoom, disconnectRoom, hasLines, clearingLines, onCopyAll, onClearAll,
}: SettingsPanelProps) {
  const { t } = useTranslation(['texthooker', 'common']);
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const settingsPanelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  const vertical = settings?.texthooker_vertical ?? false;
  const lineGap = settings?.texthooker_line_gap ?? 16;
  const lineSpacing = settings?.texthooker_line_spacing ?? 1.5;
  const fontSize = settings?.texthooker_font_size ?? 24;
  const fontFamily = settings?.texthooker_font_family ?? 'Noto Sans JP';
  const customCss = settings?.texthooker_custom_css ?? '';
  const hookerTheme = settings?.texthooker_theme ?? 'system';
  const forceTopbarVertical = settings?.texthooker_force_topbar_vertical ?? false;
  const autoPauseTimeout = settings?.texthooker_auto_pause_timeout ?? 120;
  const dictionaryPopupDelay = Math.max(0, Math.min(2000, settings?.dictionary_popup_delay_ms ?? 75));

  // Close on an outside click or Escape, like the web one.
  useEffect(() => {
    const onMouseDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (settingsButtonRef.current?.contains(target)) return;
      if (!settingsPanelRef.current?.contains(target)) onCloseRef.current();
    };
    const onEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCloseRef.current();
    };
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('keydown', onEscape);
    return () => {
      document.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('keydown', onEscape);
    };
  }, [settingsButtonRef]);

  return (
    <div
      ref={settingsPanelRef}
      className="fixed top-16 right-3 z-[60] w-[min(92vw,52rem)] max-h-[calc(100vh-5rem)] overflow-y-auto rounded-xl border border-base-content/15 bg-base-100/95 backdrop-blur-md shadow-lg p-5 th-settings-panel"
    >
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-bold text-lg flex items-center gap-2">
          <Settings className="w-5 h-5 text-primary" />
          {t('settings.title')}
        </h3>
        <button
          onClick={onClose}
          className="btn btn-ghost btn-sm btn-circle"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      <div role="tablist" className="tabs tabs-border mb-5">
        <button
          role="tab"
          className={`tab ${activeSettingsTab === 'display' ? 'tab-active' : ''}`}
          onClick={() => setActiveSettingsTab('display')}
        >
          {t('settings.tabDisplay')}
        </button>
        <button
          role="tab"
          className={`tab ${activeSettingsTab === 'behavior' ? 'tab-active' : ''}`}
          onClick={() => setActiveSettingsTab('behavior')}
        >
          {t('settings.tabBehavior')}
        </button>
        <button
          role="tab"
          className={`tab ${activeSettingsTab === 'intelligence' ? 'tab-active' : ''}`}
          onClick={() => setActiveSettingsTab('intelligence')}
        >
          {t('settings.tabIntelligence')}
        </button>
      </div>

      {activeSettingsTab === 'display' && (
        <div className="space-y-4">
          <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
            <Monitor className="w-4 h-4" /> {t('settings.displayHeading')}
          </h4>

          <div>
            <label className="label cursor-pointer">
              <span>{t('settings.verticalText')}</span>
              <input
                type="checkbox"
                className="toggle toggle-primary toggle-sm"
                checked={vertical}
                onChange={(e) => void updateSettings({ texthooker_vertical: e.target.checked })}
              />
            </label>
          </div>

          <div>
            <label className="label cursor-pointer">
              <span>{t('settings.alwaysVerticalBar')}</span>
              <input
                type="checkbox"
                className="toggle toggle-primary toggle-sm"
                checked={forceTopbarVertical}
                onChange={(e) =>
                  void updateSettings({ texthooker_force_topbar_vertical: e.target.checked })
                }
              />
            </label>
          </div>

          <div className="rounded-lg border border-base-content/10 bg-base-200/40 p-3">
            <div className="text-[11px] font-semibold uppercase tracking-wide opacity-70 mb-2">
              {t('settings.topBarMetrics')}
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('topbar.timer')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_show_timer ?? true}
                  onChange={(e) =>
                    void updateSettings({ texthooker_show_timer: e.target.checked })
                  }
                />
              </label>
              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('stats.speed')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_show_speed ?? true}
                  onChange={(e) =>
                    void updateSettings({ texthooker_show_speed: e.target.checked })
                  }
                />
              </label>
              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('stats.characters')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_show_characters ?? true}
                  onChange={(e) =>
                    void updateSettings({ texthooker_show_characters: e.target.checked })
                  }
                />
              </label>
              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('stats.lines')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_show_lines ?? true}
                  onChange={(e) =>
                    void updateSettings({ texthooker_show_lines: e.target.checked })
                  }
                />
              </label>
            </div>
          </div>

          <Field label={t('settings.fontSize')}>
            <input
              type="number"
              min="12"
              max="72"
              value={fontSize}
              onChange={(e) =>
                void updateSettings({ texthooker_font_size: Number(e.target.value) })
              }
              className="input input-sm w-full"
            />
          </Field>

          <Field label={t('settings.fontFamily')}>
            <select
              value={fontFamily}
              onChange={(e) => void updateSettings({ texthooker_font_family: e.target.value })}
              className="select select-sm w-full"
            >
              {FONT_FAMILY_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>

          <Field label={t('settings.theme')} hint={t('settings.themeHint')}>
            <select
              value={hookerTheme}
              onChange={(e) => void updateSettings({ texthooker_theme: e.target.value })}
              className="select select-sm w-full"
            >
              {HOOKER_THEME_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.labelKey ? t(option.labelKey) : option.label}
                </option>
              ))}
            </select>
          </Field>

          <div className="grid grid-cols-2 gap-2">
            <Field label={t('settings.lineHeight')}>
              <input
                type="number"
                min={0.5}
                max={4}
                step={0.1}
                value={lineSpacing}
                onChange={(e) =>
                  void updateSettings({ texthooker_line_spacing: Number(e.target.value) })
                }
                className="input input-sm w-full"
              />
            </Field>
            <Field label={t('settings.lineGap')}>
              <input
                type="number"
                min={0}
                max={96}
                step={1}
                value={lineGap}
                onChange={(e) =>
                  void updateSettings({ texthooker_line_gap: Number(e.target.value) })
                }
                className="input input-sm w-full"
              />
            </Field>
          </div>

          <div className="rounded-lg border border-base-300 bg-base-200/40 p-4 mt-2">
            <div className="flex items-center justify-between mb-2">
              <h4 className="font-semibold">{t('settings.customCss')}</h4>
              <button
                type="button"
                className="btn btn-ghost btn-xs"
                title={t('settings.classHelp')}
                onClick={onOpenClassHelp}
              >
                <HelpCircle className="w-4 h-4" />
              </button>
            </div>
            <p className="text-sm text-base-content/70 mb-3">
              {t('settings.customCssHint')}
            </p>
            <textarea
              className="textarea w-full h-56 font-mono text-xs"
              value={customCss}
              onChange={(e) => void updateSettings({ texthooker_custom_css: e.target.value })}
              placeholder={
                '.th-line-text {\n  letter-spacing: 0.02em;\n}\n\n.th-topbar {\n  border-radius: 12px;\n}'
              }
            />
            <div className="flex justify-end mt-3">
              <button
                type="button"
                className="btn btn-outline btn-sm"
                onClick={() => void updateSettings({ texthooker_custom_css: '' })}
              >
                {t('settings.clearCustomCss')}
              </button>
            </div>
          </div>
        </div>
      )}

      {activeSettingsTab === 'behavior' && (
        <div className="space-y-4">
          <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
            <Activity className="w-4 h-4" /> {t('settings.behaviorHeading')}
          </h4>

          <Field
            label={t('settings.autoPause')}
            hint={t('settings.autoPauseHint')}
          >
            <input
              type="number"
              min={0}
              max={3600}
              value={autoPauseTimeout}
              onChange={(e) =>
                void updateSettings({ texthooker_auto_pause_timeout: Number(e.target.value) })
              }
              className="input input-sm w-full"
            />
          </Field>

          <Field
            label={t('settings.dictionaryPopupDelay')}
            hint={t('settings.dictionaryPopupDelayHint')}
          >
            <input
              type="number"
              min={0}
              max={2000}
              step={25}
              value={dictionaryPopupDelay}
              onChange={(event) =>
                void updateSettings({
                  dictionary_popup_delay_ms: Math.max(0, Math.min(2000, Number(event.target.value) || 0)),
                })
              }
              className="input input-sm w-full"
            />
          </Field>

          <div className="rounded-lg border border-base-content/10 bg-base-200/40 p-3">
            <div className="text-[11px] font-semibold uppercase tracking-wide opacity-70 mb-2">
              {t('settings.runtime')}
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-2">
              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.clipboard')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_clipboard ?? false}
                  onChange={(e) =>
                    void updateSettings({ texthooker_clipboard: e.target.checked })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.allowLine')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_allow_line_during_pause ?? true}
                  onChange={(e) =>
                    void updateSettings({
                      texthooker_allow_line_during_pause: e.target.checked,
                    })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.allowClipboard')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_allow_clipboard_during_pause ?? true}
                  onChange={(e) =>
                    void updateSettings({
                      texthooker_allow_clipboard_during_pause: e.target.checked,
                    })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.autostartByLine')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_autostart_by_line ?? false}
                  onChange={(e) =>
                    void updateSettings({ texthooker_autostart_by_line: e.target.checked })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.autostartByClipboard')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_autostart_by_clipboard ?? false}
                  onChange={(e) =>
                    void updateSettings({ texthooker_autostart_by_clipboard: e.target.checked })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.pauseOnDisconnect')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_pause_on_disconnect ?? false}
                  onChange={(e) =>
                    void updateSettings({ texthooker_pause_on_disconnect: e.target.checked })
                  }
                />
              </label>

              <label className="flex items-center justify-between surface px-3 py-2 cursor-pointer">
                <span className="text-sm">{t('settings.autoconnect')}</span>
                <input
                  type="checkbox"
                  className="toggle toggle-primary toggle-sm"
                  checked={settings?.texthooker_autoconnect ?? true}
                  onChange={(e) =>
                    void updateSettings({ texthooker_autoconnect: e.target.checked })
                  }
                />
              </label>
            </div>
          </div>

          <Field label={t('settings.websocketUrl')}>
            <input
              type="text"
              value={settings?.texthooker_ws_url ?? ''}
              onChange={(e) => void updateSettings({ texthooker_ws_url: e.target.value })}
              className="input input-sm w-full"
              placeholder="ws://localhost:6677"
            />
          </Field>

          <NativeHookTextFilters />

          <Field label={t('room.title')} error={roomError}>
            {(id) => (
              <select
                id={id}
                className="select select-sm w-full"
                value={roomMode}
                onChange={(e) => {
                  disconnectRoom();
                  void updateSettings({ texthooker_room_mode: e.target.value });
                }}
              >
                <option value="local">{t('room.local')}</option>
                <option value="host">{t('room.host')}</option>
                <option value="guest">{t('room.guest')}</option>
              </select>
            )}
          </Field>

          {roomMode !== 'local' ? (
            <Field label={t('room.roomId')}>
              {(id) => (
                <div className="flex gap-2">
                  <input
                    id={id}
                    type="text"
                    className="input input-sm w-full font-mono"
                    value={roomId}
                    disabled={roomConnected}
                    placeholder={t('room.roomPlaceholder')}
                    onChange={(e) =>
                      void updateSettings({ texthooker_room_id: e.target.value })
                    }
                  />
                  {roomMode === 'host' && !roomId ? (
                    <button
                      type="button"
                      className="btn btn-sm btn-ghost"
                      onClick={() =>
                        void updateSettings({ texthooker_room_id: randomRoomId() })
                      }
                    >
                      {t('room.generate')}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className={`btn btn-sm ${roomConnected ? 'btn-error' : 'btn-primary'}`}
                    disabled={!roomId.trim()}
                    onClick={() => (roomConnected ? disconnectRoom() : connectRoom())}
                  >
                    {roomConnected
                      ? t('room.disconnect')
                      : roomMode === 'host'
                        ? t('room.hostAction')
                        : t('room.join')}
                  </button>
                </div>
              )}
            </Field>
          ) : null}

          {roomMode === 'host' && inviteLink ? (
            <Field label={t('room.inviteLink')} hint={t('room.inviteHint')}>
              {(id) => (
                <div className="flex gap-2">
                  <input
                    id={id}
                    type="text"
                    value={inviteLink}
                    readOnly
                    className="input input-sm w-full"
                  />
                  <button
                    type="button"
                    className={`btn btn-sm ${inviteCopied ? 'btn-success' : 'btn-outline'}`}
                    onClick={copyInvite}
                  >
                    {inviteCopied ? (
                      t('room.copied')
                    ) : (
                      <span className="inline-flex items-center gap-1">
                        <Share2 className="w-3.5 h-3.5" /> {t('room.copy')}
                      </span>
                    )}
                  </button>
                </div>
              )}
            </Field>
          ) : null}

          <div className="divider"></div>

          <div className="flex flex-col gap-2">
            <button
              type="button"
              onClick={onCopyAll}
              disabled={!hasLines}
              className="btn btn-outline btn-sm"
            >
              <Copy className="w-4 h-4" /> {t('settings.copyAllLines')}
            </button>
            <button
              type="button"
              onClick={onClearAll}
              disabled={!hasLines || clearingLines}
              className="btn btn-error btn-outline btn-sm"
            >
              <Trash2 className="w-4 h-4" /> {t('settings.clearAllLines')}
            </button>
          </div>
        </div>
      )}
      {activeSettingsTab === 'intelligence' && (
        <div className="space-y-4">
          <h4 className="text-sm font-semibold border-b border-base-content/10 pb-2 flex items-center gap-2">
            <Brain className="w-4 h-4 text-primary" /> {t('settings.intelligence.title')}
          </h4>
          <p className="text-sm text-base-content/70">{t('settings.intelligence.hint')}</p>
          <Field label={t('settings.intelligence.mode')}>
            <select
              className="select select-sm w-full"
              value={settings?.texthooker_intelligence_mode ?? 'automatic'}
              onChange={(event) =>
                void updateSettings({
                  texthooker_intelligence_mode: event.target.value as 'automatic' | 'manual' | 'off',
                })
              }
            >
              <option value="automatic">{t('settings.intelligence.automatic')}</option>
              <option value="manual">{t('settings.intelligence.manual')}</option>
              <option value="off">{t('settings.intelligence.off')}</option>
            </select>
          </Field>
          {settings?.texthooker_intelligence_mode === 'manual' && (
            <Field label={t('settings.intelligence.manualThreshold')}>
              <input
                type="number"
                min={1}
                max={3600}
                className="input input-sm w-full"
                value={settings.texthooker_intelligence_manual_threshold_seconds ?? 40}
                onChange={(event) =>
                  void updateSettings({
                    texthooker_intelligence_manual_threshold_seconds: Math.max(
                      1,
                      Math.min(3600, Number(event.target.value) || 1)
                    ),
                  })
                }
              />
            </Field>
          )}
          <Field label={t('settings.intelligence.afkThreshold')}>
            <input
              type="number"
              min={1}
              max={7200}
              className="input input-sm w-full"
              value={settings?.texthooker_intelligence_afk_threshold_seconds ?? 300}
              onChange={(event) =>
                void updateSettings({
                  texthooker_intelligence_afk_threshold_seconds: Math.max(
                    1,
                    Math.min(7200, Number(event.target.value) || 1)
                  ),
                })
              }
            />
          </Field>
        </div>
      )}
    </div>
  );
}
