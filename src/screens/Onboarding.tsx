import { useState, type ReactNode } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { BookA, BookOpenText, Check, ExternalLink, KeyRound, Plus, Radar, ShieldCheck, type LucideIcon } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { Field, Toggle } from '../components/ui/Field';
import { MPV_PLAYERS, MpvSetupPrompt } from '../components/MpvSetup';
import { VnAppModal } from '../components/VnAppModal';
import { useDictionaryImport } from '../hooks/useDictionaryImport';
import { ipc } from '../lib/ipc';
import {
  LANGUAGE_LABELS,
  SUPPORTED_LANGUAGES,
  type SupportedLanguage,
} from '../i18n/languages';
import type { VnApp } from '../lib/types';
import { useAppStore } from '../store/useAppStore';
import { toast } from '../store/useToastStore';

// After the account, every step can be skipped. The step lives in the URL:
// saving the key swaps the router's tree (see App), which remounts this page.
const STEPS = ['connect', 'track', 'work', 'dictionary'] as const;
type Step = (typeof STEPS)[number];

const STEP_ICONS: Record<Step, LucideIcon> = {
  connect: ShieldCheck,
  track: Radar,
  work: BookOpenText,
  dictionary: BookA,
};

function asStep(value: string | null): Step {
  return (STEPS as readonly string[]).includes(value ?? '') ? (value as Step) : 'connect';
}

export default function Onboarding() {
  const { t } = useTranslation('onboarding');
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const configured = useAppStore((s) => Boolean(s.settings?.api_key?.trim()));
  // Without a key only the first step makes sense, whatever the URL says.
  const step = configured ? asStep(params.get('step')) : 'connect';
  const index = STEPS.indexOf(step);
  const Icon = STEP_ICONS[step];

  const go = (next: Step) => setParams({ step: next }, { replace: true });
  const finish = () => navigate('/', { replace: true });
  const forward = () => (index === STEPS.length - 1 ? finish() : go(STEPS[index + 1]));

  return (
    <div className="min-h-screen flex items-center justify-center p-8 bg-base-200">
      <div className="w-full max-w-lg surface p-8">
        <ul className="steps steps-horizontal mb-6 w-full text-xs" aria-label={t('steps.label')}>
          {STEPS.map((id, position) => (
            <li
              key={id}
              className={position <= index ? 'step step-primary' : 'step'}
              aria-current={id === step ? 'step' : undefined}
            >
              {t(`steps.${id}`)}
            </li>
          ))}
        </ul>

        <div className="flex items-center gap-3">
          <div className="p-3 rounded-box bg-primary text-primary-content">
            <Icon className="w-6 h-6" />
          </div>
          <div className="min-w-0 flex-1">
            <h1 className="text-xl font-bold">{t(`pages.${step}.title`)}</h1>
            <p className="text-sm text-base-content/60">{t(`pages.${step}.subtitle`)}</p>
          </div>
          {step === 'connect' ? <LanguagePicker /> : null}
        </div>

        <div className="mt-6">
          {step === 'connect' ? <ConnectStep onConnected={() => go('track')} /> : null}
          {step === 'track' ? <StepFrame onSkip={forward} onNext={forward}><TrackStep /></StepFrame> : null}
          {step === 'work' ? <StepFrame onBack={() => go('track')} onSkip={forward} onNext={forward}><WorkStep /></StepFrame> : null}
          {step === 'dictionary' ? <StepFrame last onBack={() => go('work')} onSkip={finish} onNext={finish}><DictionaryStep /></StepFrame> : null}
        </div>
      </div>
    </div>
  );
}

function StepFrame({ children, onBack, onSkip, onNext, last = false }: {
  children: ReactNode;
  /** None on the first optional step: the account is already connected. */
  onBack?: () => void;
  onSkip: () => void;
  onNext: () => void;
  last?: boolean;
}) {
  const { t } = useTranslation(['onboarding', 'common']);
  return (
    <div className="space-y-6">
      {children}
      <div className="flex items-center gap-2 border-t border-base-300/60 pt-4">
        {onBack ? <Button appearance="ghost" size="sm" onClick={onBack}>{t('common:back')}</Button> : null}
        <span className="flex-1" />
        <Button appearance="ghost" size="sm" onClick={onSkip}>{t('nav.skip')}</Button>
        <Button variant="primary" size="sm" onClick={onNext}>{last ? t('nav.finish') : t('nav.next')}</Button>
      </div>
    </div>
  );
}

function ConnectStep({ onConnected }: { onConnected: () => void }) {
  const { t } = useTranslation('onboarding');
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const refreshConnection = useAppStore((s) => s.refreshConnection);

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
      onConnected();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-4">
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

      <Button
        variant="primary"
        block
        loading={busy}
        disabled={!apiKey.trim() || !baseUrl.trim()}
        onClick={handleConnect}
      >
        {t('connect')}
      </Button>

      <button
        type="button"
        className="btn btn-ghost btn-sm w-full"
        onClick={() => void openUrl(`${baseUrl.replace(/\/+$/, '')}/settings`)}
      >
        <ExternalLink className="w-4 h-4" />
        {t('createKey')}
      </button>
    </div>
  );
}

/** The mpv build a running process belongs to (`platform::process_key`). */
function runningMpv(names: string[]): string | null {
  const keys = names.map((name) => name.toLowerCase().replace(/\.exe$/, ''));
  if (keys.includes('mpvnet')) return 'mpv.net';
  if (keys.includes('mpv')) return 'mpv';
  return null;
}

function TrackStep() {
  const { t } = useTranslation(['onboarding', 'settings']);
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const trackedPlayer = useAppStore((s) => s.tracker?.anime?.player);
  const processes = useQuery({ queryKey: ['processes'], queryFn: ipc.listProcesses, retry: false });
  if (!settings) return null;

  const mpv = trackedPlayer && MPV_PLAYERS.includes(trackedPlayer)
    ? trackedPlayer
    : runningMpv((processes.data ?? []).map((process) => process.name));
  const offerMpv = mpv && settings.detect_players && settings.use_player_position && !settings.mpv_setup_dismissed;

  return (
    <div className="space-y-3">
      <div className="surface-muted divide-y divide-base-300/50 p-1">
        <Toggle
          label={t('settings:anime.detect')}
          description={t('settings:anime.detectHint')}
          checked={settings.detect_players}
          onChange={(v) => void updateSettings({ detect_players: v })}
        />
        <Toggle
          label={t('settings:vn.detect')}
          checked={settings.detect_visual_novels}
          onChange={(v) => void updateSettings({ detect_visual_novels: v })}
        />
        <Toggle
          label={t('settings:game.detect')}
          checked={settings.detect_games}
          onChange={(v) => void updateSettings({ detect_games: v })}
        />
      </div>
      {offerMpv ? (
        <MpvSetupPrompt player={mpv} onDismiss={() => void updateSettings({ mpv_setup_dismissed: true })} />
      ) : null}
    </div>
  );
}

function WorkStep() {
  const { t } = useTranslation(['onboarding', 'library']);
  const queryClient = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [saving, setSaving] = useState(false);
  const apps = useQuery({ queryKey: ['vn-apps'], queryFn: ipc.listVnApps });
  const works = apps.data ?? [];

  const save = async (entry: VnApp) => {
    setSaving(true);
    try {
      queryClient.setQueryData(['vn-apps'], await ipc.upsertVnApp(entry));
      setAdding(false);
      toast.success(t('library:toast.saved', { title: entry.media_title }));
    } catch (error) {
      toast.error(t('library:toast.saveFailed', { message: String(error) }));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-3">
      {works.length > 0 ? (
        <ul className="space-y-1.5">
          {works.map((work) => (
            <li key={work.exe} className="surface-muted flex items-center gap-3 px-3 py-2">
              <Check className="h-4 w-4 shrink-0 text-success" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{work.media_title}</span>
              <span className="truncate font-mono text-xs text-base-content/60">{work.exe}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <Button appearance={works.length > 0 ? 'outline' : 'solid'} variant={works.length > 0 ? undefined : 'primary'} onClick={() => setAdding(true)}>
        <Plus className="h-4 w-4" />
        {works.length > 0 ? t('work.addAnother') : t('work.add')}
      </Button>
      <p className="text-xs text-base-content/60">{t('work.animeNote')}</p>
      <VnAppModal open={adding} initial={null} onClose={() => setAdding(false)} onSave={(entry) => void save(entry)} saving={saving} />
    </div>
  );
}

function DictionaryStep() {
  const { t } = useTranslation(['onboarding', 'settings']);
  const { importDictionary, busy, progress } = useDictionaryImport();
  const dictionaries = useQuery({ queryKey: ['dictionaries'], queryFn: ipc.dictionaryList });
  const list = dictionaries.data ?? [];

  return (
    <div className="space-y-3">
      {list.length > 0 ? (
        <ul className="space-y-1.5">
          {list.map((dictionary) => (
            <li key={dictionary.id} className="surface-muted flex items-center gap-3 px-3 py-2">
              <Check className="h-4 w-4 shrink-0 text-success" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{dictionary.title}</span>
              <span className="text-xs text-base-content/60">{dictionary.termCount.toLocaleString()} {t('settings:dictionary.entries')}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <Button variant={list.length > 0 ? undefined : 'primary'} appearance={list.length > 0 ? 'outline' : 'solid'} loading={busy} onClick={() => void importDictionary()}>
        <BookA className="h-4 w-4" />
        {t('settings:dictionary.import')}
      </Button>
      {progress ? (
        <p className="truncate text-sm text-base-content/70">{t('settings:dictionary.progress', progress)}</p>
      ) : null}
      <p className="text-xs text-base-content/60">{t('dictionary.where')}</p>
    </div>
  );
}

/** Available before the account exists, so the first screen is readable too. */
function LanguagePicker() {
  const { t, i18n } = useTranslation('common');
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const current = settings?.language || i18n.language;

  return (
    <select
      className="select select-sm w-32 shrink-0"
      aria-label={t('language')}
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
