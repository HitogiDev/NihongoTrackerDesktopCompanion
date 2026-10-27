import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { EyeOff, Plus, Search, Trash2 } from 'lucide-react';
import { Button } from './ui/Button';
import { Toggle } from './ui/Field';
import { SectionTitle } from './ui/Page';
import { MediaSearchModal } from './MediaSearchModal';
import { events, ipc } from '../lib/ipc';
import { formatClock } from '../lib/format';
import type { BlacklistEntry, DiscordDecision, Settings } from '../lib/types';

/**
 * The preview is not a mock-up: it renders the decision Rust would publish,
 * taken from the same function the watcher calls. What is on screen here is
 * what other people see, so the user can check before opting in.
 */
export function DiscordSettings({
  settings,
  onChange,
}: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
}) {
  const { t } = useTranslation(['discord', 'common']);
  const queryClient = useQueryClient();
  const [picking, setPicking] = useState(false);
  const [process, setProcess] = useState('');

  const preview = useQuery({
    queryKey: ['discord-preview'],
    queryFn: ipc.discordPreview,
    // The detected session changes under it, so keep it honest.
    refetchInterval: 2000,
  });

  const blacklist = useQuery({
    queryKey: ['discord-blacklist'],
    queryFn: ipc.discordBlacklist,
  });

  useEffect(() => {
    const unlisten = listen(events.discordStatus, () => {
      void queryClient.invalidateQueries({ queryKey: ['discord-preview'] });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [queryClient]);

  const add = useMutation({
    mutationFn: (entry: BlacklistEntry) => ipc.addDiscordBlacklist(entry),
    onSuccess: (data) => {
      queryClient.setQueryData(['discord-blacklist'], data);
      void queryClient.invalidateQueries({ queryKey: ['discord-preview'] });
      setProcess('');
      setPicking(false);
    },
  });

  const remove = useMutation({
    mutationFn: (key: string) => ipc.removeDiscordBlacklist(key),
    onSuccess: (data) => {
      queryClient.setQueryData(['discord-blacklist'], data);
      void queryClient.invalidateQueries({ queryKey: ['discord-preview'] });
    },
  });

  const entries = blacklist.data ?? [];
  const decision = preview.data?.decision;

  return (
    <>
      <SectionTitle>{t('title')}</SectionTitle>
      <div className="surface p-2 divide-y divide-base-300/50">
        <Toggle
          label={t('enable')}
          description={t('enableHint')}
          checked={settings.discord_rpc}
          onChange={(v) => onChange({ discord_rpc: v })}
        />

        <div className="p-3 space-y-3">
          <p className="text-xs text-base-content/60">{t('configured')}</p>
          <p className="text-xs text-base-content/50">
            {preview.data?.connected ? t('connected') : t('disconnected')}
          </p>
        </div>

        <Toggle
          label={t('idlePresence')}
          description={t('idlePresenceHint')}
          checked={settings.discord_show_idle}
          onChange={(v) => onChange({ discord_show_idle: v })}
        />
        <Toggle
          label={t('genericMode')}
          description={t('genericModeHint')}
          checked={settings.discord_generic_mode}
          onChange={(v) => onChange({ discord_generic_mode: v })}
        />
        <Toggle
          label={t('profileButton')}
          description={t('profileButtonHint')}
          checked={settings.discord_profile_button}
          onChange={(v) => onChange({ discord_profile_button: v })}
        />

        <div className="p-3 space-y-2">
          <p className="text-sm font-medium">{t('preview.title')}</p>
          <p className="text-xs text-base-content/60">{t('preview.hint')}</p>
          <PresenceCard decision={decision} />
        </div>

        <div className="p-3 space-y-3">
          <div>
            <p className="text-sm font-medium">{t('blacklist.title')}</p>
            <p className="text-xs text-base-content/60">{t('blacklist.hint')}</p>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button size="sm" appearance="outline" onClick={() => setPicking(true)}>
              <Search className="w-4 h-4" />
              {t('blacklist.addMedia')}
            </Button>
            <label className="input input-sm flex-1 min-w-48 flex items-center gap-2">
              <input
                className="grow"
                value={process}
                placeholder={t('blacklist.processPlaceholder')}
                onChange={(e) => setProcess(e.target.value)}
              />
            </label>
            <Button
              size="sm"
              appearance="outline"
              disabled={!process.trim()}
              onClick={() =>
                add.mutate({
                  key: process.trim().toLowerCase(),
                  kind: 'process',
                  label: process.trim(),
                })
              }
            >
              <Plus className="w-4 h-4" />
              {t('common:add')}
            </Button>
          </div>

          {entries.length === 0 ? (
            <p className="text-xs text-base-content/50">{t('blacklist.empty')}</p>
          ) : (
            <ul className="space-y-1.5">
              {entries.map((entry) => (
                <li
                  key={entry.key}
                  className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-200/60"
                >
                  <EyeOff className="w-4 h-4 text-base-content/50 shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-medium truncate">{entry.label}</span>
                    <span className="block text-xs text-base-content/50 font-mono truncate">
                      {t(`blacklist.kind.${entry.kind}`)} · {entry.key}
                    </span>
                  </span>
                  <Button
                    size="xs"
                    appearance="ghost"
                    aria-label={t('common:delete')}
                    onClick={() => remove.mutate(entry.key)}
                  >
                    <Trash2 className="w-3 h-3" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <MediaSearchModal
        open={picking}
        onClose={() => setPicking(false)}
        mediaType="vn"
        onPick={(hit) =>
          add.mutate({
            key: hit.contentId,
            kind: 'media',
            label: hit.title.contentTitleRomaji || hit.title.contentTitleNative,
          })
        }
      />
    </>
  );
}

function PresenceCard({ decision }: { decision?: DiscordDecision }) {
  const { t } = useTranslation('discord');
  const [, force] = useState(0);

  // Discord counts the elapsed time itself from `startEpoch`; the preview has
  // to imitate that, so it ticks locally.
  useEffect(() => {
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  if (!decision || decision.status !== 'show') {
    const key =
      decision?.status === 'blocked'
        ? 'preview.blocked'
        : decision?.status === 'off'
          ? 'preview.off'
          : 'preview.idle';
    return (
      <div className="rounded-box border border-base-300 bg-base-200/60 p-4 text-sm text-base-content/60">
        {t(key, {
          label: decision?.status === 'blocked' ? decision.label : '',
        })}
      </div>
    );
  }

  const { presence, reason } = decision;
  const elapsed = Math.max(0, Math.floor(Date.now() / 1000) - presence.startEpoch);
  const isUrl = presence.largeImage.startsWith('http');

  return (
    <div className="rounded-box border border-base-300 bg-base-300/40 p-4 space-y-2">
      <div className="flex gap-3">
        {isUrl ? (
          <img
            src={presence.largeImage}
            alt=""
            className="w-16 h-16 rounded-box object-cover shrink-0"
          />
        ) : (
          <div className="w-16 h-16 rounded-box bg-base-100 shrink-0 flex items-center justify-center text-[10px] font-mono text-base-content/50 text-center px-1">
            {presence.largeImage || '—'}
          </div>
        )}
        <div className="min-w-0 flex-1 leading-tight">
          <p className="text-sm font-bold truncate">{presence.details}</p>
          <p className="text-sm truncate">{presence.state}</p>
          <p className="text-sm text-base-content/60">
            {t('preview.elapsed', { time: formatClock(elapsed) })}
          </p>
        </div>
      </div>
      {presence.button ? (
        <div className="btn btn-sm btn-block btn-neutral pointer-events-none">
          {presence.button.label}
        </div>
      ) : null}
      {reason !== 'full' ? (
        <p className="text-xs text-warning">{t(`preview.reason.${reason}`)}</p>
      ) : null}
    </div>
  );
}
