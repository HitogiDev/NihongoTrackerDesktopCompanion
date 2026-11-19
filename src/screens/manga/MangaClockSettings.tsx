import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Field, Toggle } from '../../components/ui/Field';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

/** A whole number in `[min, max]` from an input, or `null` when it is not one. */
export function boundedNumber(value: string, min: number, max: number): number | null {
  if (value.trim() === '') return null;
  const number = Math.round(Number(value));
  return Number.isFinite(number) ? Math.min(max, Math.max(min, number)) : null;
}

const SAVE_DELAY_MS = 500;

/**
 * A number setting that types freely: what is typed stays local and is
 * saved on blur, on Enter, or half a second after the last key, never on
 * every keystroke (each save goes through Rust and back). Out-of-range
 * values are brought into range when saved.
 */
export function NumberSetting({ label, hint, value, min, max, step = 1, onSave }: {
  label: string;
  /** Only for what the label cannot say (what 0 means). */
  hint?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onSave: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  const timer = useRef<number | null>(null);
  const editing = useRef(false);

  // Follow the saved value (another window, or the save itself) unless the
  // user is typing in this one.
  useEffect(() => {
    if (!editing.current) setDraft(String(value));
  }, [value]);
  useEffect(() => () => { if (timer.current !== null) window.clearTimeout(timer.current); }, []);

  const commit = (text: string) => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    const next = boundedNumber(text, min, max);
    if (next === null) {
      setDraft(String(value));
      return;
    }
    setDraft(String(next));
    if (next !== value) onSave(next);
  };

  return (
    <div className="p-3">
      <Field label={label} hint={hint}>
        {(id) => (
          <input
            id={id}
            type="number"
            inputMode="numeric"
            min={min}
            max={max}
            step={step}
            className="input input-sm w-28 tabular-nums"
            value={draft}
            onFocus={() => { editing.current = true; }}
            onChange={(e) => {
              const text = e.target.value;
              setDraft(text);
              if (timer.current !== null) window.clearTimeout(timer.current);
              timer.current = window.setTimeout(() => commit(text), SAVE_DELAY_MS);
            }}
            onBlur={(e) => {
              editing.current = false;
              commit(e.target.value);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commit(e.currentTarget.value);
            }}
          />
        )}
      </Field>
    </div>
  );
}

/**
 * How the manga reader's clock runs and where its sessions go. The same
 * controls in Settings → Manga reader and in the reader's clock menu, so the
 * two never say different things. Labels say what an option does; a hint
 * is only there for what a label cannot (what 0 means, a case that is not
 * obvious). Rust applies them on the next tick (`manga::session::tick`).
 */
export function MangaClockSettings({ settings }: { settings: Settings }) {
  const { t } = useTranslation('settings');
  const updateSettings = useAppStore((s) => s.updateSettings);
  const ownAfk = settings.manga_afk_timeout_secs;
  const generalAfk = Number(settings.afk_timeout_secs);
  const dwell = Number(settings.manga_page_dwell_secs);

  return (
    <div className="divide-y divide-base-300/50">
      <Toggle
        label={t('manga.autoStart')}
        checked={settings.manga_auto_start}
        onChange={(v) => void updateSettings({ manga_auto_start: v })}
      />
      {settings.manga_auto_start ? (
        <NumberSetting
          label={t('manga.autoStartDelay')}
          hint={t('manga.zeroAtOnce')}
          value={Number(settings.manga_auto_start_delay_secs)}
          min={0}
          max={600}
          onSave={(v) => void updateSettings({ manga_auto_start_delay_secs: v })}
        />
      ) : null}
      <Toggle
        label={t('manga.pauseOnUnfocus')}
        checked={settings.manga_pause_on_unfocus}
        onChange={(v) => void updateSettings({ manga_pause_on_unfocus: v })}
      />
      <Toggle
        label={generalAfk > 0 ? t('manga.afkGeneral', { count: generalAfk }) : t('manga.afkGeneralNever')}
        checked={ownAfk === null}
        onChange={(v) => void updateSettings({ manga_afk_timeout_secs: v ? null : generalAfk })}
      />
      {ownAfk !== null ? (
        <NumberSetting
          label={t('manga.afk')}
          hint={t('manga.zeroNever')}
          value={Number(ownAfk)}
          min={0}
          max={3600}
          step={5}
          onSave={(v) => void updateSettings({ manga_afk_timeout_secs: v })}
        />
      ) : null}
      <Toggle
        label={t('manga.confirm')}
        description={settings.manga_confirm_before_log ? undefined : t('manga.confirmOff')}
        checked={settings.manga_confirm_before_log}
        onChange={(v) => void updateSettings({ manga_confirm_before_log: v })}
      />
      <NumberSetting
        label={t('manga.idleLog')}
        hint={t('manga.zeroNever')}
        value={Number(settings.manga_idle_log_minutes)}
        min={0}
        max={240}
        onSave={(v) => void updateSettings({ manga_idle_log_minutes: v })}
      />
      <NumberSetting
        label={t('manga.dwell')}
        value={dwell}
        min={1}
        max={120}
        onSave={(v) => void updateSettings({ manga_page_dwell_secs: v })}
      />
    </div>
  );
}
