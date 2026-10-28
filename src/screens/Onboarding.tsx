import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ExternalLink, KeyRound, ShieldCheck } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { Field } from '../components/ui/Field';
import { ipc } from '../lib/ipc';
import {
  LANGUAGE_LABELS,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from '../i18n/languages';
import { useAppStore } from '../store/useAppStore';

export default function Onboarding() {
  const { t } = useTranslation(['onboarding', 'settings']);
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const navigate = useNavigate();

  const [baseUrl, setBaseUrl] = useState(settings?.api_base_url ?? 'https://nihongotracker.app');
  const [apiKey, setApiKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleConnect() {
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.verifyApiKey(baseUrl.trim(), apiKey.trim());
      await updateSettings({
        api_base_url: baseUrl.trim().replace(/\/+$/, ''),
        api_key: apiKey.trim(),
        username: result.username ?? null,
      });
      await refreshConnection();
      navigate('/', { replace: true });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-8 bg-base-200">
      <div className="w-full max-w-lg surface p-8">
        <div className="flex items-center gap-3">
          <div className="p-3 rounded-box bg-primary text-primary-content">
            <ShieldCheck className="w-6 h-6" />
          </div>
          <div className="min-w-0 flex-1">
            <h1 className="text-xl font-bold">{t('title')}</h1>
            <p className="text-sm text-base-content/60">{t('subtitle')}</p>
          </div>
          <LanguagePicker />
        </div>

        <div className="mt-6 space-y-4">
          <Field label={t('server')}>
            {(id) => (
              <input
                id={id}
                className="input w-full"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
                placeholder="https://nihongotracker.app"
              />
            )}
          </Field>

          <Field label={t('apiKey')} hint={t('apiKeyHint')} error={error}>
            {(id) => (
              <label className="input w-full flex items-center gap-2">
                <KeyRound className="w-4 h-4 opacity-60" />
                <input
                  id={id}
                  className="grow"
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="ntk_…"
                />
              </label>
            )}
          </Field>

          <div className="flex items-center gap-2">
            <Button
              variant="primary"
              block
              loading={busy}
              disabled={!apiKey.trim() || !baseUrl.trim()}
              onClick={handleConnect}
            >
              {t('connect')}
            </Button>
          </div>

          <button
            type="button"
            className="btn btn-ghost btn-sm w-full"
            onClick={() => void openUrl(`${baseUrl.replace(/\/+$/, '')}/settings`)}
          >
            <ExternalLink className="w-4 h-4" />
            {t('createKey')}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Available before the account exists, so the first screen is readable too. */
function LanguagePicker() {
  const { i18n } = useTranslation();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const current = settings?.language || i18n.language;

  return (
    <select
      className="select select-sm w-32 shrink-0"
      aria-label="Language"
      value={SUPPORTED_LANGUAGES.includes(current as SupportedLanguage) ? current : 'en'}
      onChange={(e) => void updateSettings({ language: e.target.value })}
    >
      {SUPPORTED_LANGUAGES.map((code) => (
        <option key={code} value={code}>
          {LANGUAGE_LABELS[code]}
        </option>
      ))}
    </select>
  );
}
