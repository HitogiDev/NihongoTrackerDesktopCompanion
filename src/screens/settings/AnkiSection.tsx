import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { RefreshCw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Field } from '../../components/ui/Field';
import { ipc } from '../../lib/ipc';
import { miningError } from '../../lib/mining';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

export function AnkiSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const updateSettings = useAppStore((s) => s.updateSettings);
  // Only asked for while this section is open: AnkiConnect may not be running.
  const ankiTarget = [settings.anki_connect_url, settings.anki_api_key];
  const ankiModels = useQuery({ queryKey: ['anki-models', ...ankiTarget], queryFn: ipc.ankiModels, retry: false });
  const ankiFields = useQuery({ queryKey: ['anki-model-fields', settings.anki_model, ...ankiTarget], queryFn: () => ipc.ankiModelFields(settings.anki_model ?? 'Basic'), retry: false });

  return (
    <>
      <div className="surface p-4 space-y-3">
        <div className="grid gap-2 lg:grid-cols-2">
          <Field label={t('anki.url')} hint={t('anki.urlHint')}>
            {(id) => <input id={id} className="input w-full" spellCheck={false} placeholder="http://127.0.0.1:8765" value={settings.anki_connect_url} onChange={(e) => void updateSettings({ anki_connect_url: e.target.value })} />}
          </Field>
          <Field label={t('anki.apiKey')} hint={t('anki.apiKeyHint')}>
            {(id) => <input id={id} type="password" autoComplete="off" className="input w-full" value={settings.anki_api_key} onChange={(e) => void updateSettings({ anki_api_key: e.target.value })} />}
          </Field>
        </div>
        <Field label={t('anki.deck')}>
          {(id) => <input id={id} className="input w-full" value={settings.anki_deck} onChange={(e) => void updateSettings({ anki_deck: e.target.value })} />}
        </Field>
        <Field label={t('anki.noteType')} hint={t('anki.noteTypeHint')}>
          {(id) => <select id={id} className="select w-full" value={settings.anki_model} onChange={(e) => void updateSettings({ anki_model: e.target.value, anki_field_sources: {} })}>
            {(ankiModels.data ?? []).map((model) => <option key={model} value={model}>{model}</option>)}
            {!(ankiModels.data ?? []).includes(settings.anki_model) ? <option value={settings.anki_model}>{settings.anki_model}</option> : null}
          </select>}
        </Field>
        {ankiModels.isError || ankiFields.isError ? <div className="flex items-start gap-3">
          <div className="flex-1 space-y-1 text-xs">
            {ankiModels.isError ? <p className="text-base-content/60">{t('anki.unavailable')}</p> : null}
            <p className="text-error selectable">{miningError(ankiModels.error ?? ankiFields.error)}</p>
          </div>
          <Button size="sm" appearance="outline" loading={ankiModels.isFetching || ankiFields.isFetching} onClick={() => { void ankiModels.refetch(); void ankiFields.refetch(); }}>
            <RefreshCw className="w-4 h-4" />
            {t('anki.retry')}
          </Button>
        </div> : null}
        <div className="grid gap-2 lg:grid-cols-2">
          {(ankiFields.data ?? []).map((field) => <Field key={field} label={t('anki.field', { field })}>
            {(id) => <select id={id} className="select w-full" value={settings.anki_field_sources[field] ?? ''} onChange={(e) => void updateSettings({ anki_field_sources: { ...settings.anki_field_sources, [field]: e.target.value } })}>
              {['word', 'reading', 'meaning', 'sentence', 'media', ''].map((source) => <option key={source || 'empty'} value={source}>{t(`anki.sources.${source || 'empty'}`)}</option>)}
            </select>}
          </Field>)}
        </div>
      </div>
    </>
  );
}
