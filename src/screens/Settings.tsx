import { useState } from 'react';
import { CheckCircle2, ExternalLink, LogOut, XCircle } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../components/ui/Button';
import { Field, Toggle } from '../components/ui/Field';
import { Page, SectionTitle } from '../components/ui/Page';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';

const THEMES = [
  'dark',
  'light',
  'system',
  'dracula',
  'night',
  'dim',
  'sunset',
  'abyss',
  'forest',
  'winter',
];

export default function SettingsScreen() {
  const settings = useAppStore((s) => s.settings);
  const online = useAppStore((s) => s.online);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const refreshConnection = useAppStore((s) => s.refreshConnection);

  const [newKey, setNewKey] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  if (!settings) return null;

  async function replaceKey() {
    if (!settings) return;
    setKeyBusy(true);
    setKeyError(null);
    try {
      const result = await ipc.verifyApiKey(settings.api_base_url, newKey.trim());
      const user = result.user as { username?: string } | undefined;
      await updateSettings({ api_key: newKey.trim(), username: user?.username ?? null });
      setNewKey('');
      await refreshConnection();
    } catch (e) {
      setKeyError(String(e));
    } finally {
      setKeyBusy(false);
    }
  }

  return (
    <Page title="Ajustes" description="Cómo se detecta, cuándo se registra y a dónde se envía.">
      <SectionTitle>Cuenta</SectionTitle>
      <div className="surface p-4 space-y-3">
        <div className="flex items-center gap-3">
          {online ? (
            <CheckCircle2 className="w-5 h-5 text-success" />
          ) : (
            <XCircle className="w-5 h-5 text-error" />
          )}
          <div className="min-w-0 flex-1">
            <p className="font-medium">{settings.username ?? 'Sin usuario'}</p>
            <p className="text-xs text-base-content/60 truncate">{settings.api_base_url}</p>
          </div>
          <Button
            size="sm"
            appearance="outline"
            onClick={() => void openUrl(`${settings.api_base_url}/settings`)}
          >
            <ExternalLink className="w-4 h-4" />
            Web
          </Button>
        </div>

        <Field label="Servidor">
          {(id) => (
            <input
              id={id}
              className="input w-full"
              value={settings.api_base_url}
              onChange={(e) => void updateSettings({ api_base_url: e.target.value })}
            />
          )}
        </Field>

        <Field label="Nueva API key" error={keyError}>
          {(id) => (
            <div className="flex gap-2">
              <input
                id={id}
                type="password"
                className="input flex-1"
                value={newKey}
                onChange={(e) => setNewKey(e.target.value)}
                placeholder="ntk_…"
              />
              <Button
                variant="primary"
                loading={keyBusy}
                disabled={!newKey.trim()}
                onClick={() => void replaceKey()}
              >
                Reemplazar
              </Button>
            </div>
          )}
        </Field>

        <Button
          appearance="ghost"
          size="sm"
          onClick={() => void updateSettings({ api_key: null, username: null })}
        >
          <LogOut className="w-4 h-4" />
          Desconectar cuenta
        </Button>
      </div>

      <SectionTitle>Anime</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label="Detectar reproductores de vídeo"
          description="mpv, mpv.net, MPC-HC/BE, VLC, PotPlayer, GOM, KMPlayer, SMPlayer."
          checked={settings.detect_players}
          onChange={(v) => void updateSettings({ detect_players: v })}
        />
        <Toggle
          label="Registrar sin confirmar"
          description="Con esto apagado el episodio espera en Pendientes hasta que lo apruebes."
          checked={settings.anime_auto_log}
          onChange={(v) => void updateSettings({ anime_auto_log: v })}
        />
        <Toggle
          label="Enviar el tiempo medido"
          description="La web ya acredita 24 min por episodio; actívalo solo si quieres el tiempo real."
          checked={settings.anime_log_time}
          onChange={(v) => void updateSettings({ anime_log_time: v })}
        />
        <div className="p-3 grid gap-3 sm:grid-cols-2">
          <Field label={`Umbral: ${settings.anime_threshold_percent}% del episodio`}>
            {(id) => (
              <input
                id={id}
                type="range"
                min={10}
                max={100}
                step={5}
                className="range range-primary"
                value={settings.anime_threshold_percent}
                onChange={(e) =>
                  void updateSettings({ anime_threshold_percent: Number(e.target.value) })
                }
              />
            )}
          </Field>
          <Field label="Duración por defecto (min)">
            {(id) => (
              <input
                id={id}
                type="number"
                min={1}
                max={180}
                className="input w-full"
                value={settings.anime_episode_minutes}
                onChange={(e) =>
                  void updateSettings({ anime_episode_minutes: Number(e.target.value) })
                }
              />
            )}
          </Field>
        </div>
      </div>

      <SectionTitle>Novelas visuales</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label="Detectar novelas registradas"
          description="Al abrir el ejecutable empieza la sesión y se abre el texthooker."
          checked={settings.detect_visual_novels}
          onChange={(v) => void updateSettings({ detect_visual_novels: v })}
        />
        <Toggle
          label="Solo contar con la ventana en primer plano"
          checked={settings.vn_require_focus}
          onChange={(v) => void updateSettings({ vn_require_focus: v })}
        />
        <Toggle
          label="Abrir el texthooker automáticamente"
          checked={settings.vn_open_texthooker}
          onChange={(v) => void updateSettings({ vn_open_texthooker: v })}
        />
        <Toggle
          label="Confirmar antes de registrar la sesión"
          description="Al cerrar la novela el registro pasa por Pendientes."
          checked={settings.vn_confirm_before_log}
          onChange={(v) => void updateSettings({ vn_confirm_before_log: v })}
        />
        <div className="p-3">
          <Field
            label={`Pausa por inactividad: ${settings.afk_timeout_secs}s`}
            hint="0 desactiva la pausa automática."
          >
            {(id) => (
              <input
                id={id}
                type="range"
                min={0}
                max={600}
                step={10}
                className="range range-primary"
                value={settings.afk_timeout_secs}
                onChange={(e) => void updateSettings({ afk_timeout_secs: Number(e.target.value) })}
              />
            )}
          </Field>
        </div>
      </div>

      <SectionTitle>Aplicación</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label="Iniciar con el sistema"
          checked={settings.autostart}
          onChange={(v) => void updateSettings({ autostart: v })}
        />
        <Toggle
          label="Arrancar minimizado"
          checked={settings.start_minimized}
          onChange={(v) => void updateSettings({ start_minimized: v })}
        />
        <Toggle
          label="Cerrar a la bandeja"
          description="La detección sigue funcionando con la ventana cerrada."
          checked={settings.minimize_to_tray}
          onChange={(v) => void updateSettings({ minimize_to_tray: v })}
        />
        <Toggle
          label="Notificaciones del sistema"
          checked={settings.notifications}
          onChange={(v) => void updateSettings({ notifications: v })}
        />
        <div className="p-3 grid gap-3 sm:grid-cols-2">
          <Field label="Tema">
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={settings.theme}
                onChange={(e) => void updateSettings({ theme: e.target.value })}
              >
                {THEMES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            )}
          </Field>
          <Field
            label={`Frecuencia de sondeo: ${settings.poll_interval_ms} ms`}
            hint="Más alto = menos CPU."
          >
            {(id) => (
              <input
                id={id}
                type="range"
                min={500}
                max={5000}
                step={250}
                className="range range-primary"
                value={settings.poll_interval_ms}
                onChange={(e) => void updateSettings({ poll_interval_ms: Number(e.target.value) })}
              />
            )}
          </Field>
        </div>
      </div>
    </Page>
  );
}
