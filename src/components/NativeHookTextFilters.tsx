import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ipc } from '../lib/ipc';
import type { FilterPreview } from '../lib/types';
import { useAppStore } from '../store/useAppStore';
import { Button } from './ui/Button';
import { Field, Toggle } from './ui/Field';

export function NativeHookTextFilters() {
  const { t } = useTranslation('settings');
  const settings = useAppStore((state) => state.settings);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const savedWords = settings?.native_hook_text_filters?.excluded_words ?? '';
  const savedPatterns = settings?.native_hook_text_filters?.regex_patterns ?? '';
  const [words, setWords] = useState(savedWords);
  const [patterns, setPatterns] = useState(savedPatterns);
  const [sample, setSample] = useState(t('filters.sample'));
  const [preview, setPreview] = useState<FilterPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setWords(savedWords);
    setPatterns(savedPatterns);
    setPreview(null);
  }, [savedWords, savedPatterns]);

  const changed = () => {
    setPreview(null);
    setError('');
    setSaved(false);
  };

  const run = async (save: boolean) => {
    setBusy(true);
    setError('');
    setSaved(false);
    try {
      const filters = { excluded_words: words, regex_patterns: patterns };
      const result = await ipc.previewNativeHookFilters(
        filters, sample, settings?.texthooker_prevent_duplicates ?? false, settings?.texthooker_filter_chinese ?? false,
      );
      setPreview(result);
      if (result.regex_error) return;
      if (save) {
        await updateSettings({ native_hook_text_filters: filters });
        setSaved(true);
      }
    } catch (cause) {
      setError(t('filters.failed', { message: String(cause) }));
    } finally {
      setBusy(false);
    }
  };

  if (!settings) return null;

  return <section className="surface space-y-3 p-4" aria-label={t('filters.title')}>
    <h3 className="font-semibold">{t('filters.title')}</h3>
    <Toggle label={t('filters.duplicates')} description={t('filters.duplicatesHint')}
      checked={settings.texthooker_prevent_duplicates} disabled={busy}
      onChange={(value) => {
        changed();
        void updateSettings({ texthooker_prevent_duplicates: value }).catch((cause) => {
          setError(t('filters.failed', { message: String(cause) }));
        });
      }} />
    <Toggle label={t('filters.chinese')} description={t('filters.chineseHint')}
      checked={settings.texthooker_filter_chinese ?? false} disabled={busy}
      onChange={(value) => {
        changed();
        void updateSettings({ texthooker_filter_chinese: value }).catch((cause) => {
          setError(t('filters.failed', { message: String(cause) }));
        });
      }} />
    <p className="text-sm text-base-content/60">{t('filters.scope')}</p>
    <div className="grid gap-3 lg:grid-cols-2">
      <Field label={t('filters.words')} hint={t('filters.wordsHint')}>
        {(id) => <textarea id={id} rows={4} className="textarea w-full" disabled={busy}
          placeholder={t('filters.wordsPlaceholder')} value={words}
          onChange={(event) => { setWords(event.target.value); changed(); }} />}
      </Field>
      <Field label={t('filters.regex')} hint={t('filters.regexHint')}
        error={preview?.regex_error ? t('filters.regexError', preview.regex_error) : null}>
        {(id) => <textarea id={id} rows={4} className="textarea w-full font-mono" disabled={busy}
          placeholder={t('filters.regexPlaceholder')} value={patterns}
          onChange={(event) => { setPatterns(event.target.value); changed(); }} />}
      </Field>
    </div>
    <details className="space-y-2">
      <summary className="cursor-pointer text-sm font-medium">{t('filters.test')}</summary>
      <Field label={t('filters.input')}>
        {(id) => <textarea id={id} rows={2} className="textarea w-full" disabled={busy} value={sample}
          onChange={(event) => { setSample(event.target.value); changed(); }} />}
      </Field>
      <Button size="sm" appearance="outline" disabled={busy} onClick={() => void run(false)}>{t('filters.preview')}</Button>
      {preview && !preview.regex_error && <div className="surface-muted space-y-1 p-3" role="status">
        <p className="whitespace-pre-wrap break-words">{preview.text || t('filters.empty')}</p>
        <p className="text-xs text-base-content/60">{t('filters.characters', { count: preview.chars })}</p>
      </div>}
    </details>
    {error && <p role="alert" className="break-words text-sm text-error">{error}</p>}
    <div className="flex flex-wrap items-center gap-3">
      <Button size="sm" variant="primary" loading={busy} disabled={words === savedWords && patterns === savedPatterns}
        onClick={() => void run(true)}>{t('filters.save')}</Button>
      {saved && <span role="status" className="text-sm text-success">{t('filters.saved')}</span>}
    </div>
  </section>;
}
