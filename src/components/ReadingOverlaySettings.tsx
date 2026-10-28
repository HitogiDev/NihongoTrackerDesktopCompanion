import { NativeHookTextFilters } from './NativeHookTextFilters';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useReadingOverlay } from '../hooks/useReadingOverlay';
import { useAppStore } from '../store/useAppStore';
import { ipc } from '../lib/ipc';
import { readingOverlayTextStyle } from '../lib/readingOverlayStyle';
import { Field, Toggle } from './ui/Field';
import { Button } from './ui/Button';
import { SectionTitle } from './ui/Page';

export function ReadingOverlaySettings() {
  const { t } = useTranslation('readingOverlay');
  const settings = useAppStore((state) => state.settings);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const view = useReadingOverlay();
  const preferences = settings?.reading_overlay;
  const [toggleShortcut, setToggleShortcut] = useState(preferences?.toggle_shortcut ?? '');
  const [interactionShortcut, setInteractionShortcut] = useState(preferences?.interaction_shortcut ?? '');
  const [error, setError] = useState(false);
  useEffect(() => {
    setToggleShortcut(preferences?.toggle_shortcut ?? '');
    setInteractionShortcut(preferences?.interaction_shortcut ?? '');
  }, [preferences?.toggle_shortcut, preferences?.interaction_shortcut]);
  if (!preferences) return null;
  const save = (patch: Partial<typeof preferences>) => {
    setError(false);
    const current = useAppStore.getState().settings?.reading_overlay ?? preferences;
    void updateSettings({ reading_overlay: { ...current, ...patch } }).catch(() => setError(true));
  };
  return <>
    <SectionTitle>{t('title')}</SectionTitle>
    <div id="reading-overlay-settings" className="surface space-y-3 p-4">
      <p className="text-sm text-base-content/70">{t('settingsHint')}</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('fontSize')}>{(id) => <input id={id} type="number" min={12} max={80} className="input w-full" value={preferences.font_size} onChange={(event) => {
          const value = event.currentTarget.valueAsNumber;
          if (Number.isFinite(value)) save({ font_size: value });
        }} />}</Field>
        <Field label={t('opacity')}>{(id) => <input id={id} type="number" min={0} max={100} className="input w-full" value={preferences.opacity} onChange={(event) => {
          const value = event.currentTarget.valueAsNumber;
          if (Number.isFinite(value)) save({ opacity: value });
        }} />}</Field>
      </div>
      <Field label={t('fontFamily')}>{(id) => <input id={id} type="text" className="input w-full" defaultValue={preferences.font_family} onBlur={(event) => save({ font_family: event.currentTarget.value.trim() || 'Noto Sans JP Local' })} />}</Field>
      <div className="surface-muted space-y-3 p-3">
        <h3 className="text-sm font-semibold">{t('textStyle.title')}</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('textStyle.outlineWidth')} hint={t('textStyle.outlineHint')}>{(id) => <input id={id} type="number" min={0} max={6} step={0.5} className="input w-full" value={preferences.text_outline_width ?? 0} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ text_outline_width: value });
          }} />}</Field>
          <Field label={t('textStyle.outlineColor')}>{(id) => <input id={id} type="color" className="input w-full cursor-pointer" value={preferences.text_outline_color ?? '#ffffff'} onChange={(event) => save({ text_outline_color: event.currentTarget.value })} />}</Field>
          <Field label={t('textStyle.shadowColor')}>{(id) => <input id={id} type="color" className="input w-full cursor-pointer" value={preferences.text_shadow_color ?? '#000000'} onChange={(event) => save({ text_shadow_color: event.currentTarget.value })} />}</Field>
          <Field label={t('textStyle.shadowOpacity')} hint={t('textStyle.shadowHint')}>{(id) => <input id={id} type="number" min={0} max={100} className="input w-full" value={preferences.text_shadow_opacity ?? 0} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ text_shadow_opacity: value });
          }} />}</Field>
          <Field label={t('textStyle.shadowBlur')}>{(id) => <input id={id} type="number" min={0} max={40} className="input w-full" value={preferences.text_shadow_blur ?? 6} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ text_shadow_blur: value });
          }} />}</Field>
        </div>
        <p className="text-xs text-base-content/60">{t('textStyle.preview')}</p>
        <p lang="ja" className="break-words leading-relaxed" style={readingOverlayTextStyle(preferences)}>{t('textStyle.sample')}</p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('fontWeight')}>{(id) => <select id={id} className="select w-full" value={preferences.font_weight} onChange={(event) => save({ font_weight: Number(event.currentTarget.value) as typeof preferences.font_weight })}>
          {[400, 500, 600, 700].map((weight) => <option key={weight} value={weight}>{t(`fontWeights.${weight}`)}</option>)}
        </select>}</Field>
        <Field label={t('textVisibility')}>{(id) => <select id={id} className="select w-full" value={preferences.text_visibility} onChange={(event) => save({ text_visibility: event.currentTarget.value as typeof preferences.text_visibility })}>
          <option value="always">{t('visibilityModes.always')}</option>
          <option value="hover_only">{t('visibilityModes.hoverOnly')}</option>
          <option value="dim_when_idle">{t('visibilityModes.dimWhenIdle')}</option>
        </select>}</Field>
      </div>
      <Field label={t('idleOpacity')} hint={t('idleOpacityHint')}>{(id) => <input id={id} type="number" min={0} max={100} className="input w-full" disabled={preferences.text_visibility !== 'dim_when_idle'} value={preferences.idle_opacity} onChange={(event) => {
        const value = event.currentTarget.valueAsNumber;
        if (Number.isFinite(value)) save({ idle_opacity: value });
      }} />}</Field>
      <div className="space-y-1">
        <Toggle label={t('centerText')} checked={preferences.center_text} onChange={(value) => save({ center_text: value })} />
        <Toggle label={t('alwaysOnTop')} checked={preferences.always_on_top} onChange={(value) => save({ always_on_top: value })} />
        <Toggle label={t('hideWhenUnfocused')} description={t('hideWhenUnfocusedHint')} checked={preferences.hide_when_unfocused ?? true} onChange={(value) => save({ hide_when_unfocused: value })} />
        <Toggle label={t('toolbarTitleHoverOnly')} checked={preferences.toolbar_title_hover_only} onChange={(value) => save({ toolbar_title_hover_only: value })} />
      </div>
      <section aria-labelledby="overlay-dictionary-settings" className="surface-muted space-y-3 p-3">
        <h3 id="overlay-dictionary-settings" className="text-sm font-semibold">{t('popup.title')}</h3>
        <Toggle label={t('popup.enabled')} checked={preferences.popup_enabled ?? true} onChange={(value) => save({ popup_enabled: value })} />
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('popup.opacity')} hint={t('popup.opacityHint')}>{(id) => <input id={id} type="number" min={0} max={100} className="input w-full" value={preferences.popup_background_opacity ?? 100} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ popup_background_opacity: value });
          }} />}</Field>
          <Field label={t('lookupDelay')} hint={t('lookupDelayHint')}>{(id) => <input id={id} type="number" min={0} max={2000} step={25} className="input w-full" value={preferences.lookup_delay_ms} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ lookup_delay_ms: value });
          }} />}</Field>
          <Field label={t('popup.closeDelay')} hint={t('popup.closeDelayHint')}>{(id) => <input id={id} type="number" min={0} max={5000} step={50} className="input w-full" value={preferences.popup_close_delay_ms ?? 500} onChange={(event) => {
            const value = event.currentTarget.valueAsNumber;
            if (Number.isFinite(value)) save({ popup_close_delay_ms: value });
          }} />}</Field>
        </div>
      </section>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t('toggleShortcut')} hint={t('shortcutHint')}>{(id) => <input id={id} className="input w-full" value={toggleShortcut} onChange={(event) => setToggleShortcut(event.currentTarget.value)} onBlur={() => save({ toggle_shortcut: toggleShortcut.trim() })} />}</Field>
        <Field label={t('interactionShortcut')} hint={t('shortcutHint')}>{(id) => <input id={id} className="input w-full" value={interactionShortcut} onChange={(event) => setInteractionShortcut(event.currentTarget.value)} onBlur={() => save({ interaction_shortcut: interactionShortcut.trim() })} />}</Field>
      </div>
      {(view?.shortcut_errors ?? []).map((shortcut) => <p key={shortcut} role="alert" className="text-sm text-error">{t('shortcutError', { shortcut })}</p>)}
      {error && <p role="alert" className="text-sm text-error">{t('saveFailed')}</p>}
      <div className="flex flex-wrap gap-2">
        <Button appearance="outline" size="sm" disabled={!view?.target} onClick={() => void ipc.readingOverlayAction('toggle')}>{t('toggle')}</Button>
        <Button appearance="outline" size="sm" disabled={!view?.target} onClick={() => void ipc.readingOverlayAction('interaction')}>{t(view?.click_through ? 'enableInteraction' : 'enableClickThrough')}</Button>
      </div>
    </div>
    <NativeHookTextFilters />
  </>;
}
