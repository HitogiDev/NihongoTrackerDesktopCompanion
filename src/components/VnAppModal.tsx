import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { FolderOpen, Search } from 'lucide-react';
import { open as openFile } from '@tauri-apps/plugin-dialog';
import { Button } from './ui/Button';
import { Field, Toggle } from './ui/Field';
import { Modal } from './ui/Modal';
import { MediaSearchModal } from './MediaSearchModal';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';
import type { MediaHit, ProcessEntry, VnApp } from '../lib/types';

const EMPTY: VnApp = {
  exe: '',
  media_id: '',
  media_type: 'vn',
  media_title: '',
  image: null,
  launch_path: null,
  hooker_path: null,
  native_hook_enabled: false,
  native_hook_code: null,
  reader_mode: 'integrated',
  enabled: true,
  is_adult: false,
  is_adult_image: null,
  filter_chinese: null,
};

/** Add or edit a library work. Shared by the Library and the onboarding. */
export function VnAppModal({
  open,
  initial,
  onClose,
  onSave,
  saving,
}: {
  open: boolean;
  initial: VnApp | null;
  onClose: () => void;
  onSave: (entry: VnApp) => void;
  saving: boolean;
}) {
  const { t } = useTranslation(['library', 'common']);
  // Windows programs are .exe files; elsewhere any file (or a macOS .app) runs.
  const os = useAppStore((s) => s.tracker?.capabilities.os);
  const nativeHookAvailable = useAppStore((s) => s.tracker?.capabilities.native_hook ?? false);
  const globalChineseFilter = useAppStore((s) => s.settings?.texthooker_filter_chinese ?? false);
  const programFilters = os === 'windows' ? [{ name: t('modal.programFilter'), extensions: ['exe'] }] : undefined;
  const [draft, setDraft] = useState<VnApp>(EMPTY);
  const [picking, setPicking] = useState(false);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (open) setDraft(initial ?? EMPTY);
  }, [open, initial]);

  function applyMedia(hit: MediaHit, type: string) {
    setDraft((d) => ({
      ...d,
      media_id: hit.contentId,
      media_type: hit.type ?? type,
      media_title: hit.title.contentTitleRomaji || hit.title.contentTitleNative,
      image: hit.contentImage ?? hit.coverImage,
      // Carried along so Discord Rich Presence can strip the title later.
      is_adult: hit.isAdult,
      is_adult_image: hit.isAdultImage ?? null,
    }));
    setSearching(false);
  }

  const executableName = draft.exe.trim() || getExecutableName(draft.launch_path);
  const valid = executableName && draft.media_id.trim() && draft.media_title.trim();

  return (
    <>
      <Modal
        open={open}
        onClose={onClose}
        title={initial ? t('modal.editTitle') : t('modal.addTitle')}
        size="lg"
        actions={
          <>
            <Button appearance="ghost" onClick={onClose}>
              {t('common:cancel')}
            </Button>
            <Button
              variant="primary"
              loading={saving}
              disabled={!valid}
              onClick={() => onSave({ ...draft, exe: executableName.toLowerCase() })}
            >
              {t('common:save')}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label={t('modal.kind')}>
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={draft.media_type === 'game' ? 'game' : 'vn'}
                onChange={(e) => setDraft({ ...draft, media_type: e.target.value, media_id: '', media_title: '', image: null })}
              >
                <option value="vn">{t('common:logType.vn')}</option>
                <option value="game">{t('common:logType.game')}</option>
              </select>
            )}
          </Field>
          <Field label={t('modal.executable')} hint={t('modal.executableHint')}>
            {(id) => (
              <div className="flex gap-2">
                <input
                  id={id}
                  className="input flex-1 font-mono text-sm"
                  value={draft.exe}
                  onChange={(e) => setDraft({ ...draft, exe: e.target.value })}
                  placeholder={draft.launch_path ? getExecutableName(draft.launch_path) || t('modal.executablePlaceholder') : t('modal.executablePlaceholder')}
                />
                <Button appearance="outline" onClick={() => setPicking(true)}>
                  <Search className="w-4 h-4" />
                  {t('modal.choose')}
                </Button>
              </div>
            )}
          </Field>

          <Field label={t('modal.launchPath')} hint={t('modal.launchPathHint')}>
            {(id) => <div className="flex gap-2"><input id={id} className="input min-w-0 flex-1 font-mono text-sm" value={draft.launch_path ?? ''} onChange={(e) => setDraft({ ...draft, launch_path: e.target.value || null })} placeholder="C:\\Games\\game.exe" /><Button type="button" appearance="outline" aria-label={t('modal.choose')} onClick={async () => { const path = await openFile({ multiple: false, directory: false, filters: programFilters }); if (typeof path === 'string') setDraft({ ...draft, launch_path: path }); }}><FolderOpen className="h-4 w-4" /></Button></div>}
          </Field>
          <Field label={t('modal.hookerPath')} hint={t('modal.hookerPathHint')}>
            {(id) => <div className="flex gap-2"><input id={id} className="input min-w-0 flex-1 font-mono text-sm" value={draft.hooker_path ?? ''} onChange={(e) => setDraft({ ...draft, hooker_path: e.target.value || null })} placeholder="C:\\Tools\\Textractor.exe" /><Button type="button" appearance="outline" aria-label={t('modal.choose')} onClick={async () => { const path = await openFile({ multiple: false, directory: false, filters: [{ name: t('modal.hookerFilter'), extensions: ['exe', 'bat', 'cmd'] }] }); if (typeof path === 'string') setDraft({ ...draft, hooker_path: path }); }}><FolderOpen className="h-4 w-4" /></Button></div>}
          </Field>

          {nativeHookAvailable && (draft.media_type === 'game' || draft.media_type === 'vn') ? (
            <Toggle
              label={t('modal.nativeHook')}
              description={t('modal.nativeHookHint')}
              checked={draft.native_hook_enabled}
              disabled={draft.reader_mode === 'overlay'}
              onChange={(native_hook_enabled) => setDraft({ ...draft, native_hook_enabled })}
            />
          ) : null}

          <Field label={t('modal.work')}>
            {() => (
              <div className="flex items-center gap-3 p-3 rounded-field bg-base-200/60">
                {draft.image ? (
                  <img src={draft.image} alt="" className="w-10 h-14 object-cover rounded" />
                ) : (
                  <div className="w-10 h-14 rounded bg-base-300" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="font-medium truncate">
                    {draft.media_title || t('modal.notSelected')}
                  </p>
                  <p className="text-xs text-base-content/60 font-mono">
                    {draft.media_id || '—'}
                  </p>
                </div>
                <Button appearance="outline" size="sm" onClick={() => setSearching(true)}>
                  {t('common:search')}
                </Button>
              </div>
            )}
          </Field>

          <Field label={t('modal.readerMode')} hint={draft.reader_mode === 'overlay' ? t('modal.overlayHint') : undefined}>
            <div className="join flex flex-wrap" role="group" aria-label={t('modal.readerMode')}>
              {(['integrated', 'browser', 'overlay'] as const).filter((mode) => mode !== 'overlay' || draft.media_type === 'vn').map((mode) => (
                <button key={mode} type="button" aria-pressed={draft.reader_mode === mode}
                  className={draft.reader_mode === mode ? 'join-item btn btn-primary btn-sm' : 'join-item btn btn-outline btn-sm'}
                  onClick={() => setDraft({ ...draft, reader_mode: mode, native_hook_enabled: mode === 'overlay' || draft.native_hook_enabled })}>
                  {t(`modal.readerModes.${mode}`)}
                </button>
              ))}
            </div>
          </Field>

          <Field label={t('modal.filterChinese')} hint={t('modal.filterChineseHint')}>
            <div className="join flex flex-wrap" role="group" aria-label={t('modal.filterChinese')}>
              {([null, true, false] as const).map((value) => (
                <button key={String(value)} type="button" aria-pressed={draft.filter_chinese === value}
                  className={draft.filter_chinese === value ? 'join-item btn btn-primary btn-sm' : 'join-item btn btn-outline btn-sm'}
                  onClick={() => setDraft({ ...draft, filter_chinese: value })}>
                  {value === null
                    ? t('modal.filterChineseModes.global', { state: t(globalChineseFilter ? 'modal.filterChineseModes.on' : 'modal.filterChineseModes.off') })
                    : t(value ? 'modal.filterChineseModes.on' : 'modal.filterChineseModes.off')}
                </button>
              ))}
            </div>
          </Field>

          <Toggle
            label={t('modal.enabled')}
            checked={draft.enabled}
            onChange={(enabled) => setDraft({ ...draft, enabled })}
          />
        </div>
      </Modal>

      <ProcessPickerModal
        open={picking}
        onClose={() => setPicking(false)}
        onPick={(entry) => {
          setDraft((d) => ({ ...d, exe: entry.name }));
          setPicking(false);
        }}
      />

      <MediaSearchModal
        open={searching}
        onClose={() => setSearching(false)}
        mediaType={draft.media_type || 'vn'}
        allowTypeChange={false}
        initialQuery={draft.media_title}
        onPick={applyMedia}
      />
    </>
  );
}

function getExecutableName(path: string | null) {
  const parts = path?.trim().split(/[\\/]/).filter(Boolean) ?? [];
  // A macOS bundle `Novel.app` runs as the process `Novel`.
  return (parts[parts.length - 1] ?? '').replace(/\.app$/i, '');
}

function ProcessPickerModal({
  open,
  onClose,
  onPick,
}: {
  open: boolean;
  onClose: () => void;
  onPick: (entry: ProcessEntry) => void;
}) {
  const { t } = useTranslation(['library', 'common']);
  const [filter, setFilter] = useState('');
  const { data, isFetching, refetch } = useQuery({
    queryKey: ['processes'],
    queryFn: ipc.listProcesses,
    enabled: open,
  });

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = data ?? [];
    if (!q) return list;
    return list.filter(
      (p) => p.name.includes(q) || (p.title ?? '').toLowerCase().includes(q)
    );
  }, [data, filter]);

  return (
    <Modal open={open} onClose={onClose} title={t('processes.title')} size="lg">
      <div className="space-y-3">
        <div className="flex gap-2">
          <label className="input input-sm flex-1 flex items-center gap-2">
            <Search className="w-4 h-4 opacity-60" />
            <input
              className="grow"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t('processes.filter')}
            />
          </label>
          <Button size="sm" appearance="outline" loading={isFetching} onClick={() => void refetch()}>
            {t('common:refresh')}
          </Button>
        </div>
        <ul className="max-h-96 overflow-y-auto space-y-1">
          {filtered.map((entry) => (
            <li key={`${entry.name}-${entry.pid}`}>
              <button
                type="button"
                className="flex w-full items-center gap-3 rounded-field px-3 py-2 text-left transition-colors hover:bg-base-200 cursor-pointer"
                onClick={() => onPick(entry)}
              >
                <span className="min-w-0 flex-1">
                  <span className="block font-mono text-sm truncate">{entry.name}</span>
                  {entry.title ? (
                    <span className="block text-xs text-base-content/60 truncate">
                      {entry.title}
                    </span>
                  ) : null}
                </span>
                {entry.title ? (
                  <span className="badge badge-xs badge-primary">{t('processes.window')}</span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
