import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { BookOpenText, Play, Plus, RefreshCw, Search, Settings as SettingsIcon, type LucideIcon } from 'lucide-react';
import { Modal } from './ui/Modal';
import { COMMUNITY_NAV, TRACKER_NAV } from './navItems';
import { shortcutLabel } from '../lib/desktop';
import { ipc } from '../lib/ipc';
import { matchesSearch } from '../lib/search';
import { useAppStore } from '../store/useAppStore';
import { useQuickLogStore } from '../store/useQuickLogStore';
import { toast } from '../store/useToastStore';

type Group = 'sections' | 'actions' | 'works';

interface Command {
  id: string;
  group: Group;
  label: string;
  /** Extra words it can be found by (the work's executable, say). */
  keywords?: string;
  icon: LucideIcon;
  shortcut?: string;
  run: () => void | Promise<unknown>;
}

const GROUPS: Group[] = ['sections', 'actions', 'works'];

/** Ctrl/⌘+K: every section, the common actions and the library's works. */
export default function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation(['nav', 'common']);
  const navigate = useNavigate();
  const openQuickLog = useQuickLogStore((s) => s.openForm);
  const refreshConnection = useAppStore((s) => s.refreshConnection);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const works = useQuery({ queryKey: ['vn-apps'], queryFn: ipc.listVnApps, enabled: open });

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setActive(0);
    // showModal() focuses the first control on its own; make sure it is this one.
    requestAnimationFrame(() => input.current?.focus());
  }, [open]);

  const commands = useMemo<Command[]>(() => {
    const sections = [...TRACKER_NAV, ...COMMUNITY_NAV].map<Command>((item) => ({
      id: `section:${item.to}`,
      group: 'sections',
      label: t(item.labelKey),
      icon: item.icon,
      shortcut: item.shortcut,
      run: () => navigate(item.to),
    }));
    const actions: Command[] = [
      { id: 'log', group: 'actions', label: t('palette.log'), icon: Plus, shortcut: 'L', run: () => openQuickLog('manual') },
      { id: 'texthooker', group: 'actions', label: t('openTexthooker'), icon: BookOpenText, shortcut: 'T', run: () => ipc.openWorkTexthooker() },
      {
        id: 'sync',
        group: 'actions',
        label: t('palette.sync'),
        icon: RefreshCw,
        run: async () => {
          try {
            await ipc.flushOutbox();
            await refreshConnection();
            toast.success(t('dashboard:toast.synced'));
          } catch (error) {
            toast.error(t('dashboard:toast.syncFailed', { message: String(error) }));
          }
        },
      },
      { id: 'settings', group: 'actions', label: t('settings'), icon: SettingsIcon, shortcut: ',', run: () => ipc.openAuxWindow('settings') },
    ];
    const library = (works.data ?? []).flatMap<Command>((work) => [
      ...(work.launch_path ? [{
        id: `launch:${work.exe}`,
        group: 'works' as const,
        label: t('palette.launch', { title: work.media_title }),
        keywords: work.exe,
        icon: Play,
        run: () => ipc.launchProgram(work.launch_path!).catch((error) =>
          toast.error(t('library:toast.launchFailed', { message: String(error) }))),
      }] : []),
      {
        id: `texthooker:${work.exe}`,
        group: 'works' as const,
        label: t('palette.workTexthooker', { title: work.media_title }),
        keywords: work.exe,
        icon: BookOpenText,
        run: () => ipc.openWorkTexthooker(work.media_id),
      },
    ]);
    return [...sections, ...actions, ...library];
  }, [t, navigate, openQuickLog, refreshConnection, works.data]);

  const visible = useMemo(
    () => commands.filter((command) => matchesSearch(`${command.label} ${command.keywords ?? ''}`, query)),
    [commands, query],
  );
  const selected = Math.min(active, Math.max(0, visible.length - 1));

  useEffect(() => {
    list.current?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const runCommand = (command: Command | undefined) => {
    if (!command) return;
    onClose();
    void command.run();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (visible.length === 0) return;
      const step = event.key === 'ArrowDown' ? 1 : -1;
      setActive((selected + step + visible.length) % visible.length);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      runCommand(visible[selected]);
    }
  };

  let index = -1;
  return (
    <Modal open={open} onClose={onClose} hideClose size="lg" className="p-0">
      <div className="flex items-center gap-3 border-b border-base-300/60 px-4">
        <Search className="h-4 w-4 shrink-0 text-base-content/50" />
        <input
          ref={input}
          autoFocus
          role="combobox"
          aria-expanded="true"
          aria-controls="command-palette-list"
          aria-activedescendant={visible[selected] ? `command-${selected}` : undefined}
          aria-label={t('palette.placeholder')}
          className="h-12 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-base-content/50"
          placeholder={t('palette.placeholder')}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        <kbd className="kbd kbd-sm">Esc</kbd>
      </div>
      <ul ref={list} id="command-palette-list" role="listbox" aria-label={t('palette.title')} className="max-h-96 overflow-y-auto p-2">
        {visible.length === 0 ? (
          <li className="px-3 py-8 text-center text-sm text-base-content/60">{t('palette.empty')}</li>
        ) : null}
        {GROUPS.map((group) => {
          const items = visible.filter((command) => command.group === group);
          if (items.length === 0) return null;
          return (
            <li key={group} role="presentation">
              <p className="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.16em] text-base-content/45">
                {t(`palette.groups.${group}`)}
              </p>
              <ul role="presentation">
                {items.map((command) => {
                  index += 1;
                  const position = index;
                  const Icon = command.icon;
                  const isActive = position === selected;
                  return (
                    <li
                      key={command.id}
                      id={`command-${position}`}
                      data-index={position}
                      role="option"
                      aria-selected={isActive}
                      className={isActive
                        ? 'flex cursor-pointer items-center gap-3 rounded-field bg-base-200 px-3 py-2 text-sm text-base-content'
                        : 'flex cursor-pointer items-center gap-3 rounded-field px-3 py-2 text-sm text-base-content/80'}
                      onMouseMove={() => { if (!isActive) setActive(position); }}
                      onClick={() => runCommand(command)}
                    >
                      <Icon className="h-4 w-4 shrink-0 text-base-content/60" />
                      <span className="min-w-0 flex-1 truncate">{command.label}</span>
                      {command.shortcut ? <kbd className="kbd kbd-xs">{shortcutLabel(command.shortcut)}</kbd> : null}
                    </li>
                  );
                })}
              </ul>
            </li>
          );
        })}
      </ul>
    </Modal>
  );
}
