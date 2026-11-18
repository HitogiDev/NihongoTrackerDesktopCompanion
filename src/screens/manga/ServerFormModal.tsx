import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CircleAlert, CircleCheck, Globe, KeyRound, PlugZap, ShieldCheck } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Field } from '../../components/ui/Field';
import { Modal } from '../../components/ui/Modal';
import { ipc } from '../../lib/ipc';
import type { MangaSource, WebdavAuth, WebdavProbe } from '../../lib/types';
import { toast } from '../../store/useToastStore';
import { webdavErrorText } from './useWebdav';

type AuthKind = WebdavAuth['kind'];
const AUTH_KINDS: AuthKind[] = ['none', 'basic', 'bearer'];
type Result = { ok: true; probe: WebdavProbe } | { ok: false; message: string };

/**
 * Adds or edits a WebDAV source. The credentials go to Rust and stay there:
 * editing a source shows only what kind they are, and empty fields keep them.
 */
export function ServerFormModal({ open, onClose, source }: {
  open: boolean;
  onClose: () => void;
  /** The source being edited; a new one otherwise. */
  source?: MangaSource | null;
}) {
  const { t } = useTranslation(['manga', 'common']);
  const editing = Boolean(source);
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [kind, setKind] = useState<AuthKind>('none');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [token, setToken] = useState('');
  const [result, setResult] = useState<Result | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(source?.name ?? '');
    setUrl(source?.url ?? '');
    setKind((source?.auth as AuthKind | undefined) ?? 'none');
    setUsername('');
    setPassword('');
    setToken('');
    setResult(null);
  }, [open, source]);

  // Editing with the same kind and nothing typed keeps the stored credentials.
  const keepStored = editing && kind === source?.auth && kind !== 'none' && !username && !password && !token;
  const auth: WebdavAuth | null = keepStored
    ? null
    : kind === 'basic'
      ? { kind, username, password }
      : kind === 'bearer'
        ? { kind, token }
        : { kind: 'none' };
  const incomplete = !url.trim() || (!keepStored && ((kind === 'basic' && !username) || (kind === 'bearer' && !token)));

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult({ ok: true, probe: await ipc.mangaWebdavTest(url, auth, source?.id) });
    } catch (error) {
      setResult({ ok: false, message: webdavErrorText(error) });
    } finally {
      setTesting(false);
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      await ipc.mangaWebdavSave(name, url, auth, source?.id);
      toast.success(editing ? t('webdav.toast.updated') : t('webdav.toast.added'));
      onClose();
    } catch (error) {
      setResult({ ok: false, message: webdavErrorText(error) });
    } finally {
      setSaving(false);
    }
  };

  const keepHint = editing && kind === source?.auth && kind !== 'none' ? t('webdav.form.keepHint') : undefined;

  return (
    <Modal
      open={open}
      onClose={onClose}
      size="lg"
      title={editing ? t('webdav.form.editTitle') : t('webdav.form.title')}
      actions={
        <>
          <Button appearance="ghost" size="sm" onClick={onClose}>{t('common:cancel')}</Button>
          <Button appearance="outline" size="sm" loading={testing} disabled={incomplete || saving} onClick={() => void test()}>
            {testing ? null : <PlugZap className="h-4 w-4" />}
            {t('webdav.form.test')}
          </Button>
          <Button variant="primary" size="sm" loading={saving} disabled={incomplete || testing} onClick={() => void save()}>
            {editing ? t('webdav.form.save') : t('webdav.form.add')}
          </Button>
        </>
      }
    >
      <p className="-mt-2 mb-2 text-sm text-base-content/60">{t('webdav.form.body')}</p>
      <form className="space-y-1" onSubmit={(event) => { event.preventDefault(); if (!incomplete) void test(); }}>
        <Field label={t('webdav.form.url')} hint={t('webdav.form.urlHint')}>
          {(id) => (
            <label className="input w-full" htmlFor={id}>
              <Globe className="h-4 w-4 opacity-50" aria-hidden="true" />
              <input
                id={id}
                type="url"
                inputMode="url"
                autoComplete="off"
                spellCheck={false}
                placeholder="https://"
                value={url}
                onChange={(event) => { setUrl(event.target.value); setResult(null); }}
                className="font-mono text-sm"
              />
            </label>
          )}
        </Field>
        <Field label={t('webdav.form.name')} aside={t('webdav.form.optional')}>
          {(id) => (
            <input id={id} className="input w-full" value={name} onChange={(event) => setName(event.target.value)} placeholder={t('webdav.form.namePlaceholder')} />
          )}
        </Field>
        <Field label={t('webdav.form.auth')}>
          <div role="radiogroup" aria-label={t('webdav.form.auth')} className="tabs tabs-box tabs-sm w-fit">
            {AUTH_KINDS.map((option) => (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={kind === option}
                className={kind === option ? 'tab tab-active' : 'tab'}
                onClick={() => { setKind(option); setResult(null); }}
              >
                {t(`webdav.form.authKind.${option}`)}
              </button>
            ))}
          </div>
        </Field>
        {kind === 'basic' ? (
          <div className="grid gap-x-3 sm:grid-cols-2">
            <Field label={t('webdav.form.username')}>
              {(id) => <input id={id} className="input w-full" autoComplete="username" value={username} placeholder={keepHint ? t('webdav.form.unchanged') : undefined} onChange={(event) => { setUsername(event.target.value); setResult(null); }} />}
            </Field>
            <Field label={t('webdav.form.password')}>
              {(id) => <input id={id} type="password" className="input w-full" autoComplete="current-password" value={password} placeholder={keepHint ? t('webdav.form.unchanged') : undefined} onChange={(event) => { setPassword(event.target.value); setResult(null); }} />}
            </Field>
          </div>
        ) : null}
        {kind === 'bearer' ? (
          <Field label={t('webdav.form.token')}>
            {(id) => (
              <label className="input w-full" htmlFor={id}>
                <KeyRound className="h-4 w-4 opacity-50" aria-hidden="true" />
                <input id={id} type="password" autoComplete="off" value={token} placeholder={keepHint ? t('webdav.form.unchanged') : undefined} onChange={(event) => { setToken(event.target.value); setResult(null); }} className="font-mono text-sm" />
              </label>
            )}
          </Field>
        ) : null}
        {keepHint ? <p className="text-xs text-base-content/55">{keepHint}</p> : null}
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </form>

      <div aria-live="polite" className="mt-4 min-h-14">
        {result?.ok ? (
          <div className="surface-muted flex items-start gap-3 border border-success/30 px-4 py-3">
            <CircleCheck className="mt-0.5 h-5 w-5 shrink-0 text-success" aria-hidden="true" />
            <div className="min-w-0 text-sm">
              <p className="font-semibold">{t('webdav.form.found', { count: result.probe.series })}</p>
              <p className="mt-0.5 truncate text-base-content/60">
                {result.probe.bunko ? t('webdav.form.foundBunko', { path: result.probe.library_path }) : t('webdav.form.foundAt', { path: result.probe.library_path })}
              </p>
            </div>
          </div>
        ) : result ? (
          <div role="alert" className="surface-muted flex items-start gap-3 border border-error/30 px-4 py-3">
            <CircleAlert className="mt-0.5 h-5 w-5 shrink-0 text-error" aria-hidden="true" />
            <p className="text-sm">{result.message}</p>
          </div>
        ) : (
          <p className="flex items-start gap-2 px-1 py-2 text-xs text-base-content/55">
            <ShieldCheck className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            {t('webdav.form.privacy')}
          </p>
        )}
      </div>
    </Modal>
  );
}
