import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, ExternalLink, LogOut, RefreshCw, XCircle } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../../components/ui/Button';
import { Field, Toggle } from '../../components/ui/Field';
import { ipc } from '../../lib/ipc';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

export function AccountSection({ settings }: { settings: Settings }) {
  const { t } = useTranslation(['settings', 'common']);
  const online = useAppStore((s) => s.online);
  const user = useAppStore((s) => s.user);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const [newKey, setNewKey] = useState('');
  const [keyBusy, setKeyBusy] = useState(false);
  const [reconnectBusy, setReconnectBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  async function replaceKey() {
    setKeyBusy(true);
    setKeyError(null);
    try {
      const result = await ipc.verifyApiKey(settings.api_base_url, newKey.trim());
      await updateSettings({ api_key: newKey.trim(), username: result.username ?? null });
      setNewKey('');
      await refreshConnection();
    } catch (e) {
      setKeyError(String(e));
    } finally {
      setKeyBusy(false);
    }
  }

  async function reconnect() {
    setReconnectBusy(true);
    try {
      await refreshConnection();
    } finally {
      setReconnectBusy(false);
    }
  }

  return (
    <>
      <div className="surface p-4 space-y-3">
        <div className="flex items-center gap-3">
          {online ? (
            <CheckCircle2 className="w-5 h-5 text-success" />
          ) : (
            <XCircle className="w-5 h-5 text-error" />
          )}
          <div className="min-w-0 flex-1">
            <p className="font-medium">{user?.username ?? settings.username ?? t('account.noUser')}</p>
            <p className="text-xs text-base-content/60">
              {online === null ? t('account.checking') : online ? t('account.connected') : t('account.disconnected')}
            </p>
            <p className="text-xs text-base-content/60 truncate">{settings.api_base_url}</p>
          </div>
          <Button
            size="sm"
            appearance="outline"
            onClick={() => void openUrl(`${settings.api_base_url}/settings`)}
          >
            <ExternalLink className="w-4 h-4" />
            {t('account.web')}
          </Button>
        </div>

        <Field label={t('account.server')}>
          {(id) => (
            <input
              id={id}
              className="input w-full"
              value={settings.api_base_url}
              onChange={(e) => void updateSettings({ api_base_url: e.target.value })}
            />
          )}
        </Field>

        {settings.api_key ? (
          <Field label={t('account.currentKey')}>
            {(id) => (
              <input
                id={id}
                type="password"
                className="input w-full"
                value={'x'.repeat(24)}
                readOnly
                autoComplete="off"
              />
            )}
          </Field>
        ) : null}

        <Field label={t('account.newKey')} error={keyError}>
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
                {t('account.replace')}
              </Button>
            </div>
          )}
        </Field>

        <div className="flex flex-wrap items-center gap-3">
          <Button appearance="outline" size="sm" loading={reconnectBusy} disabled={!settings.api_key} onClick={() => void reconnect()}>
            <RefreshCw className="h-4 w-4" />
            {t('account.reconnect')}
          </Button>
        </div>
        <Toggle
          label={t('account.autoReconnect')}
          checked={settings.auto_reconnect}
          onChange={(value) => void updateSettings({ auto_reconnect: value })}
        />

        <Button
          appearance="ghost"
          size="sm"
          onClick={() => void updateSettings({ api_key: null, username: null })}
        >
          <LogOut className="w-4 h-4" />
          {t('account.signOut')}
        </Button>
      </div>
    </>
  );
}
