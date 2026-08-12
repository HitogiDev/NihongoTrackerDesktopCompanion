import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ExternalLink, KeyRound, ShieldCheck } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { Field } from '../components/ui/Field';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';

export default function Onboarding() {
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
      const user = result.user as { username?: string } | undefined;
      await updateSettings({
        api_base_url: baseUrl.trim().replace(/\/+$/, ''),
        api_key: apiKey.trim(),
        username: user?.username ?? null,
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
          <div>
            <h1 className="text-xl font-bold">Conecta tu cuenta</h1>
            <p className="text-sm text-base-content/60">
              El companion usa una API key, no tu contraseña.
            </p>
          </div>
        </div>

        <div className="mt-6 space-y-4">
          <Field label="Servidor">
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

          <Field
            label="API key"
            hint="Ajustes → API keys en NihongoTracker. Empieza por ntk_ y solo se muestra una vez."
            error={error}
          >
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
              Conectar
            </Button>
          </div>

          <button
            type="button"
            className="btn btn-ghost btn-sm w-full"
            onClick={() => void openUrl(`${baseUrl.replace(/\/+$/, '')}/settings`)}
          >
            <ExternalLink className="w-4 h-4" />
            Crear una API key
          </button>
        </div>
      </div>
    </div>
  );
}
