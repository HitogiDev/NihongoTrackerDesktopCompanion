import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { ArrowUpCircle, Check, Download, Puzzle, RefreshCw, Search, SearchX, ShieldAlert, Trash2 } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import DropdownSelect from '../../../components/ui/DropdownSelect';
import { EmptyState, ErrorState } from '../../../components/ui/Empty';
import { ipc } from '../../../lib/ipc';
import { mangaUrl } from '../../../lib/manga';
import type { ExtAction, ExtExtension } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { toast } from '../../../store/useToastStore';
import { EXT_SOURCES_QUERY, EXT_STORE_QUERY, extErrorText } from './useExtensions';

type Show = 'all' | 'installed' | 'updates';
const SHOWS: Show[] = ['all', 'installed', 'updates'];

/** An extension's icon from the server, through `manga://`; a quiet placeholder until then. */
export function ExtIcon({ token, className = 'h-11 w-11' }: { token: string | null; className?: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <span className={`grid shrink-0 place-items-center overflow-hidden rounded-field bg-base-200 text-base-content/30 ring-1 ring-base-content/5 ${className}`}>
      {token && !failed ? (
        <img src={mangaUrl.ext(token)} alt="" loading="lazy" decoding="async" draggable={false} onError={() => setFailed(true)} className="h-full w-full object-cover" />
      ) : (
        <Puzzle className="h-1/2 w-1/2" aria-hidden="true" />
      )}
    </span>
  );
}

/** SAFE, MIXED or NSFW, as the store marks it. */
export function ContentBadge({ content }: { content: string }) {
  const { t } = useTranslation('manga');
  const tone = content === 'safe' ? 'badge-success' : content === 'mixed' ? 'badge-warning' : 'badge-error';
  return <span className={`badge badge-soft badge-xs shrink-0 ${tone}`} title={t(`ext.content.${content}Hint`)}>{t(`ext.content.${content}`)}</span>;
}

/** The language of an extension, by name: `ja` → "Japanese". */
export function languageName(code: string, locale: string): string {
  if (code === 'all') return code;
  try {
    return new Intl.DisplayNames([locale], { type: 'language' }).of(code) ?? code;
  } catch {
    return code;
  }
}

function ExtensionCard({ extension, busy, onAction }: { extension: ExtExtension; busy: ExtAction | null; onAction: (action: ExtAction) => void }) {
  const { t, i18n } = useTranslation('manga');
  return (
    <li className="surface flex min-w-0 items-center gap-3 p-3 motion-safe:transition-shadow hover:shadow-lg">
      <ExtIcon token={extension.icon} />
      <div className="min-w-0 flex-1">
        <p className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-sm font-semibold">{extension.name}</span>
          <ContentBadge content={extension.content} />
        </p>
        <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-xs text-base-content/55">
          <span>{languageName(extension.lang, i18n.language)}</span>
          <span aria-hidden="true">·</span>
          <span className="font-mono">v{extension.version}</span>
          {extension.obsolete ? <span className="badge badge-ghost badge-xs">{t('ext.store.obsolete')}</span> : null}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {extension.has_update ? (
          <Button variant="primary" size="sm" loading={busy === 'update'} disabled={busy !== null} onClick={() => onAction('update')}>
            {busy === 'update' ? null : <ArrowUpCircle className="h-4 w-4" />}
            {t('ext.store.update')}
          </Button>
        ) : null}
        {extension.installed ? (
          <>
            {!extension.has_update ? (
              <span className="badge badge-success badge-soft badge-sm gap-1"><Check className="h-3 w-3" aria-hidden="true" />{t('ext.store.installed')}</span>
            ) : null}
            <Button appearance="ghost" size="sm" shape="square" loading={busy === 'uninstall'} disabled={busy !== null} aria-label={t('ext.store.uninstall', { name: extension.name })} title={t('ext.store.uninstall', { name: extension.name })} onClick={() => onAction('uninstall')}>
              {busy === 'uninstall' ? null : <Trash2 className="h-4 w-4" />}
            </Button>
          </>
        ) : (
          <Button appearance="outline" size="sm" loading={busy === 'install'} disabled={busy !== null} onClick={() => onAction('install')}>
            {busy === 'install' ? null : <Download className="h-4 w-4" />}
            {t('ext.store.install')}
          </Button>
        )}
      </div>
    </li>
  );
}

/** The Keiyoushi store: search, filters, and install / update / uninstall with live state. */
export function ExtStore() {
  const { t, i18n } = useTranslation('manga');
  const queryClient = useQueryClient();
  const showAdult = useAppStore((s) => s.settings?.manga_ext_show_adult ?? false);
  const store = useQuery({ queryKey: [...EXT_STORE_QUERY, showAdult], queryFn: ipc.mangaExtExtensions, staleTime: 5 * 60_000 });
  const [lang, setLang] = useState('ja');
  const [show, setShow] = useState<Show>('all');
  const [search, setSearch] = useState('');
  const [busy, setBusy] = useState<Record<string, ExtAction>>({});

  const languages = useMemo(() => {
    const codes = new Set((store.data ?? []).map((extension) => extension.lang));
    codes.add('ja');
    return [...codes].sort((a, b) => (a === 'ja' ? -1 : b === 'ja' ? 1 : languageName(a, i18n.language).localeCompare(languageName(b, i18n.language))));
  }, [store.data, i18n.language]);

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (store.data ?? []).filter((extension) =>
      (lang === '*' || extension.lang === lang || extension.lang === 'all')
      && (show !== 'installed' || extension.installed)
      && (show !== 'updates' || extension.has_update)
      && (!needle || extension.name.toLowerCase().includes(needle)));
  }, [store.data, lang, show, search]);

  const act = async (extension: ExtExtension, action: ExtAction) => {
    setBusy((current) => ({ ...current, [extension.pkg_name]: action }));
    try {
      const updated = await ipc.mangaExtUpdateExtension(extension.pkg_name, action);
      queryClient.setQueryData<ExtExtension[]>([...EXT_STORE_QUERY, showAdult], (list) =>
        list?.map((item) => item.pkg_name === extension.pkg_name
          ? updated ?? { ...item, installed: action !== 'uninstall', has_update: false }
          : item));
      await queryClient.invalidateQueries({ queryKey: EXT_SOURCES_QUERY });
      toast.success(t(`ext.store.done.${action}`, { name: extension.name }));
    } catch (error) {
      toast.error(t('ext.store.failed', { name: extension.name, message: extErrorText(error) }));
    } finally {
      setBusy((current) => {
        const next = { ...current };
        delete next[extension.pkg_name];
        return next;
      });
    }
  };

  const updates = (store.data ?? []).filter((extension) => extension.has_update).length;

  return (
    <div className="space-y-4">
      <p className="surface-muted flex items-start gap-2.5 px-4 py-3 text-xs text-base-content/65">
        <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-warning" aria-hidden="true" />
        <span>{t('ext.store.warning')}{showAdult ? null : <> {t('ext.store.adultHidden')}</>}</span>
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <label className="input input-sm min-w-48 flex-1">
          <Search className="h-4 w-4 opacity-60" />
          <input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('ext.store.search')} />
        </label>
        <DropdownSelect className="select select-sm w-44" aria-label={t('ext.store.language')} value={lang} onChange={(event) => setLang(event.target.value)}>
          <option value="*">{t('ext.store.allLanguages')}</option>
          {languages.map((code) => <option key={code} value={code}>{languageName(code, i18n.language)}</option>)}
        </DropdownSelect>
        <div role="tablist" aria-label={t('ext.store.show')} className="tabs tabs-box tabs-sm">
          {SHOWS.map((id) => (
            <button key={id} type="button" role="tab" aria-selected={show === id} className={show === id ? 'tab tab-active gap-1.5' : 'tab gap-1.5'} onClick={() => setShow(id)}>
              {t(`ext.store.filter.${id}`)}
              {id === 'updates' && updates > 0 ? <span className="badge badge-primary badge-xs">{updates}</span> : null}
            </button>
          ))}
        </div>
        <Button appearance="ghost" size="sm" shape="square" loading={store.isFetching} aria-label={t('ext.store.refresh')} title={t('ext.store.refresh')} onClick={() => void store.refetch()}>
          {store.isFetching ? null : <RefreshCw className="h-4 w-4" />}
        </Button>
      </div>
      {store.isPending ? (
        <ul className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3" aria-busy="true">
          {Array.from({ length: 9 }, (_, index) => <li key={index} className="skeleton h-[4.25rem] rounded-box bg-base-100" />)}
        </ul>
      ) : store.isError ? (
        <div className="surface"><ErrorState title={t('ext.store.loadFailed')} error={extErrorText(store.error)} retrying={store.isFetching} onRetry={() => void store.refetch()} /></div>
      ) : filtered.length === 0 ? (
        <div className="surface">
          <EmptyState icon={SearchX} title={t('ext.store.none')} description={show === 'all' ? t('ext.store.noneBody') : undefined} />
        </div>
      ) : (
        <>
          <p className="text-xs text-base-content/50">{t('ext.store.count', { count: filtered.length })}</p>
          <ul className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
            {filtered.map((extension) => (
              <ExtensionCard key={extension.pkg_name} extension={extension} busy={busy[extension.pkg_name] ?? null} onAction={(action) => void act(extension, action)} />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
