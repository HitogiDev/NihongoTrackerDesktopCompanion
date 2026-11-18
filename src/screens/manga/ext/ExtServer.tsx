import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { CircleAlert, CircleCheck, Download, Globe, PlugZap, Power, Puzzle, RefreshCw, Server, ShieldCheck, Store } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Field } from '../../../components/ui/Field';
import { Modal } from '../../../components/ui/Modal';
import { ipc } from '../../../lib/ipc';
import { displayUrl } from '../../../lib/manga';
import type { ExtStatus, WebdavAuth } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { toast } from '../../../store/useToastStore';
import { MANGA } from '../Cover';
import { EXT_STATUS_QUERY, extErrorText, isInstalling, useExtInstallStore } from './useExtensions';

/** The saved custom server, as the library lists its source (never its credentials). */
export function useCustomServer() {
  return useQuery({
    queryKey: ['manga', 'ext', 'custom'],
    queryFn: async () => (await ipc.mangaSources()).find((source) => source.id === 'suwayomi-custom') ?? null,
  });
}

/**
 * The user's own Suwayomi-Server: its address and, if it uses basic auth,
 * a user and password. They go to Rust and stay there: empty fields keep
 * what is saved.
 */
export function ExtCustomModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation(['manga', 'common']);
  const queryClient = useQueryClient();
  const saved = useCustomServer().data ?? null;
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [url, setUrl] = useState('');
  const [basic, setBasic] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<'test' | 'save' | null>(null);

  useEffect(() => {
    if (!open) return;
    setUrl(saved?.url ?? '');
    setBasic(saved?.auth === 'basic');
    setUsername('');
    setPassword('');
    setResult(null);
  }, [open, saved]);

  const keepStored = basic && saved?.auth === 'basic' && !username && !password;
  const auth: WebdavAuth | null = keepStored ? null : basic ? { kind: 'basic', username, password } : { kind: 'none' };
  const incomplete = !url.trim() || (basic && !keepStored && !username);

  const run = async (kind: 'test' | 'save') => {
    setBusy(kind);
    setResult(null);
    try {
      const version = kind === 'test' ? await ipc.mangaExtTestCustom(url, auth) : await ipc.mangaExtSaveCustom(url, auth);
      if (kind === 'save') {
        if (settings && settings.manga_ext_mode !== 'custom') await updateSettings({ manga_ext_mode: 'custom' });
        await queryClient.invalidateQueries({ queryKey: ['manga', 'ext'] });
        toast.success(t('ext.custom.saved', { version }));
        onClose();
      } else {
        setResult({ ok: true, text: t('ext.custom.found', { version }) });
      }
    } catch (error) {
      setResult({ ok: false, text: extErrorText(error) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      size="lg"
      title={t('ext.custom.title')}
      actions={
        <>
          <Button appearance="ghost" size="sm" onClick={onClose}>{t('common:cancel')}</Button>
          <Button appearance="outline" size="sm" loading={busy === 'test'} disabled={incomplete || busy !== null} onClick={() => void run('test')}>
            {busy === 'test' ? null : <PlugZap className="h-4 w-4" />}
            {t('ext.custom.test')}
          </Button>
          <Button variant="primary" size="sm" loading={busy === 'save'} disabled={incomplete || busy !== null} onClick={() => void run('save')}>
            {t('ext.custom.save')}
          </Button>
        </>
      }
    >
      <p className="-mt-2 mb-2 text-sm text-base-content/60">{t('ext.custom.body')}</p>
      <form className="space-y-1" onSubmit={(event) => { event.preventDefault(); if (!incomplete) void run('test'); }}>
        <Field label={t('ext.custom.url')} hint={t('ext.custom.urlHint')}>
          {(id) => (
            <label className="input w-full" htmlFor={id}>
              <Globe className="h-4 w-4 opacity-50" aria-hidden="true" />
              <input id={id} type="url" inputMode="url" autoComplete="off" spellCheck={false} placeholder="http://192.168.1.10:4567" value={url} onChange={(event) => { setUrl(event.target.value); setResult(null); }} className="font-mono text-sm" />
            </label>
          )}
        </Field>
        <label className="flex cursor-pointer items-center gap-3 px-1 py-2 text-sm">
          <input type="checkbox" className="toggle toggle-sm toggle-primary" checked={basic} onChange={(event) => { setBasic(event.target.checked); setResult(null); }} />
          {t('ext.custom.basic')}
        </label>
        {basic ? (
          <div className="grid gap-x-3 sm:grid-cols-2">
            <Field label={t('webdav.form.username')}>
              {(id) => <input id={id} className="input w-full" autoComplete="username" value={username} placeholder={saved?.auth === 'basic' ? t('webdav.form.unchanged') : undefined} onChange={(event) => { setUsername(event.target.value); setResult(null); }} />}
            </Field>
            <Field label={t('webdav.form.password')}>
              {(id) => <input id={id} type="password" className="input w-full" autoComplete="current-password" value={password} placeholder={saved?.auth === 'basic' ? t('webdav.form.unchanged') : undefined} onChange={(event) => { setPassword(event.target.value); setResult(null); }} />}
            </Field>
          </div>
        ) : null}
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </form>
      <div aria-live="polite" className="mt-4 min-h-14">
        {result ? (
          <div role={result.ok ? 'status' : 'alert'} className={result.ok ? 'surface-muted flex items-start gap-3 border border-success/30 px-4 py-3' : 'surface-muted flex items-start gap-3 border border-error/30 px-4 py-3'}>
            {result.ok ? <CircleCheck className="mt-0.5 h-5 w-5 shrink-0 text-success" aria-hidden="true" /> : <CircleAlert className="mt-0.5 h-5 w-5 shrink-0 text-error" aria-hidden="true" />}
            <p className="text-sm">{result.text}</p>
          </div>
        ) : (
          <p className="flex items-start gap-2 px-1 py-2 text-xs text-base-content/55">
            <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            {t('ext.custom.privacy')}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Before extensions can be used: install the managed server, or point at your own. */
export function ExtSetup({ status }: { status: ExtStatus }) {
  const { t } = useTranslation('manga');
  const show = useExtInstallStore((s) => s.show);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const [customOpen, setCustomOpen] = useState(false);
  const installing = isInstalling(status);
  const percent = status.install && Number(status.install.total) > 0 ? Math.min(100, Math.round((Number(status.install.received) / Number(status.install.total)) * 100)) : 0;

  return (
    <section className="surface relative isolate overflow-hidden">
      <div className={`absolute -right-16 -top-16 -z-10 h-56 w-56 rounded-full blur-3xl ${MANGA.bgColor}`} aria-hidden="true" />
      <div className="grid gap-6 p-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <div className="space-y-3">
          <span className={`grid h-12 w-12 place-items-center rounded-box ${MANGA.bgColor} ${MANGA.color}`}><Puzzle className="h-6 w-6" aria-hidden="true" /></span>
          <h2 className="text-xl font-bold leading-tight">{t('ext.setup.title')}</h2>
          <p className="text-sm text-base-content/70">{t('ext.setup.body')}</p>
          <p className="text-xs text-base-content/55">{t('ext.setup.thirdParty')}</p>
        </div>
        <div className="grid gap-3">
          <div className={status.mode === 'managed' ? 'rounded-box border-2 border-primary bg-primary/5 p-4' : 'rounded-box border-2 border-base-300 p-4'}>
            <div className="flex items-center justify-between gap-2">
              <p className="font-semibold">{t('ext.mode.managed')}</p>
              <span className="badge badge-primary badge-soft badge-sm">{t('ext.mode.recommended')}</span>
            </div>
            <p className="mt-1 text-xs text-base-content/60">{t('ext.mode.managedHint')}</p>
            {installing ? (
              <div className="mt-3 space-y-1">
                <div className="flex justify-between text-xs tabular-nums text-base-content/60"><span>{t(`ext.install.phase.${status.install!.phase}`)}</span><span>{percent}%</span></div>
                <progress className="progress progress-primary h-1.5 w-full" value={percent} max={100} aria-label={t('ext.install.progress')} />
                <Button appearance="ghost" size="xs" onClick={show}>{t('ext.install.show')}</Button>
              </div>
            ) : (
              <Button
                variant="primary"
                size="sm"
                className="mt-3"
                disabled={!status.supported}
                onClick={async () => {
                  if (status.mode !== 'managed') await updateSettings({ manga_ext_mode: 'managed' });
                  show();
                }}
              >
                <Download className="h-4 w-4" />
                {t('ext.setup.install')}
              </Button>
            )}
            {!status.supported ? <p className="mt-2 text-xs text-warning">{t('ext.errors.unsupported')}</p> : null}
          </div>
          <div className={status.mode === 'custom' ? 'rounded-box border-2 border-primary bg-primary/5 p-4' : 'rounded-box border-2 border-base-300 p-4'}>
            <p className="font-semibold">{t('ext.mode.custom')}</p>
            <p className="mt-1 text-xs text-base-content/60">{t('ext.mode.customHint')}</p>
            <Button appearance="outline" size="sm" className="mt-3" onClick={() => setCustomOpen(true)}>
              <Server className="h-4 w-4" />
              {t('ext.setup.custom')}
            </Button>
          </div>
        </div>
      </div>
      <ExtCustomModal open={customOpen} onClose={() => setCustomOpen(false)} />
    </section>
  );
}

/** One line on the server: starting, ready (with its version), or why not. */
export function ExtServerStrip({ status, onRetry, retrying }: { status: ExtStatus; onRetry: () => void; retrying: boolean }) {
  const { t } = useTranslation('manga');
  const queryClient = useQueryClient();
  const custom = useCustomServer().data;
  const [adding, setAdding] = useState(false);
  const [stopping, setStopping] = useState(false);
  const ready = status.phase === 'ready';
  const failed = status.phase === 'error';
  const where = status.mode === 'custom' && custom ? displayUrl(custom.url) : t('ext.server.managed');

  const addStore = async () => {
    setAdding(true);
    try {
      await ipc.mangaExtAddStore();
      await queryClient.invalidateQueries({ queryKey: ['manga', 'ext'] });
    } catch (error) {
      toast.error(extErrorText(error));
    } finally {
      setAdding(false);
    }
  };

  return (
    <div className={failed ? 'surface-muted flex flex-wrap items-center gap-3 border border-error/30 px-4 py-2.5' : 'surface flex flex-wrap items-center gap-3 px-4 py-2.5'}>
      <span className={failed ? 'grid h-8 w-8 shrink-0 place-items-center rounded-field bg-error/15 text-error' : `grid h-8 w-8 shrink-0 place-items-center rounded-field ${MANGA.bgColor} ${MANGA.color}`}>
        {status.phase === 'starting' ? <span className="loading loading-spinner loading-xs" aria-hidden="true" /> : <Server className="h-4 w-4" aria-hidden="true" />}
      </span>
      <div className="min-w-0 flex-1" aria-live="polite">
        <p className="truncate text-sm font-medium">
          Suwayomi{status.version ? <span className="ml-1.5 font-mono text-xs text-base-content/50">{status.version}</span> : null}
          <span className="ml-1.5 text-xs font-normal text-base-content/50">· {where}</span>
        </p>
        <p className={failed ? 'text-xs text-error' : 'text-xs text-base-content/60'}>
          {failed ? extErrorText(status.error) : t(`ext.server.phase.${status.phase}`)}
        </p>
      </div>
      {ready && status.store_ready === false ? (
        <Button appearance="outline" size="sm" loading={adding} onClick={() => void addStore()}>
          {adding ? null : <Store className="h-4 w-4" />}
          {t('ext.server.addStore')}
        </Button>
      ) : null}
      {failed || status.phase === 'stopped' ? (
        <Button appearance="outline" size="sm" loading={retrying} onClick={onRetry}>
          {retrying ? null : <RefreshCw className="h-4 w-4" />}
          {failed ? t('ext.server.retry') : t('ext.server.start')}
        </Button>
      ) : null}
      {ready && status.mode === 'managed' ? (
        <Button
          appearance="ghost"
          size="sm"
          shape="square"
          loading={stopping}
          aria-label={t('ext.server.stop')}
          title={t('ext.server.stop')}
          onClick={async () => {
            setStopping(true);
            try {
              await ipc.mangaExtStop();
              await queryClient.invalidateQueries({ queryKey: EXT_STATUS_QUERY });
            } finally {
              setStopping(false);
            }
          }}
        >
          {stopping ? null : <Power className="h-4 w-4" />}
        </Button>
      ) : null}
    </div>
  );
}
