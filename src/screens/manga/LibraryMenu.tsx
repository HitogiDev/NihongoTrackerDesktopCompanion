import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { FolderCog, FolderPlus, Plug, Puzzle, Server, Settings2, SlidersHorizontal, type LucideIcon } from 'lucide-react';
import type { MangaSource } from '../../lib/types';
import { formatCount } from '../../lib/manga';

export type Origin = 'all' | 'local' | 'webdav' | 'extensions';

/** The settings section the app opens on next (see `Settings.tsx`). */
const SETTINGS_SECTION_KEY = 'settings:section';

interface Item {
  id: string;
  icon: LucideIcon;
  label: string;
  hint?: string;
  run: () => void;
}

interface Group {
  id: string;
  title: string;
  items: Item[];
}

/** What the menu offers on each tab: only what makes sense there. */
export function menuGroups(origin: Origin): Array<'local' | 'webdav' | 'extensions'> {
  switch (origin) {
    case 'local': return ['local'];
    case 'webdav': return ['webdav'];
    case 'extensions': return ['extensions'];
    default: return ['local', 'webdav', 'extensions'];
  }
}

/**
 * The Manga screen's one settings button: library folders, WebDAV servers and
 * extensions, each only on the tabs where it means something. A menu button
 * (`aria-haspopup="menu"`): arrows move, Home/End jump, Esc closes and gives
 * the focus back, Tab leaves.
 */
export function LibraryMenu({ origin, sources, adding, onFolders, onAddFolder, onServers, onAddServer, onExtensions }: {
  origin: Origin;
  sources: MangaSource[];
  adding: boolean;
  onFolders: () => void;
  onAddFolder: () => void;
  onServers: () => void;
  onAddServer: () => void;
  onExtensions: () => void;
}) {
  const { t, i18n } = useTranslation('manga');
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const folders = sources.filter((source) => source.kind === 'local').length;
  const servers = sources.filter((source) => source.kind === 'webdav').length;

  const close = (focus = true) => {
    setOpen(false);
    if (focus) button.current?.focus();
  };
  const choose = (run: () => void) => () => { close(false); run(); };

  const settingsItem: Item = {
    id: 'settings',
    icon: SlidersHorizontal,
    label: origin === 'extensions' ? t('menu.extSettings') : t('menu.readerSettings'),
    run: () => {
      try { localStorage.setItem(SETTINGS_SECTION_KEY, 'manga'); } catch { /* private storage: the first section */ }
      navigate('/settings');
    },
  };

  const groups: Group[] = menuGroups(origin).map((kind): Group => {
    if (kind === 'local') {
      return {
        id: 'local',
        title: t('menu.local'),
        items: [
          { id: 'folders', icon: FolderCog, label: t('menu.manageFolders'), hint: formatCount(folders, i18n.language), run: onFolders },
          { id: 'add-folder', icon: FolderPlus, label: adding ? t('menu.addingFolder') : t('menu.addFolder'), run: onAddFolder },
        ],
      };
    }
    if (kind === 'webdav') {
      const items: Item[] = servers > 0
        ? [{ id: 'servers', icon: Server, label: t('menu.manageServers'), hint: formatCount(servers, i18n.language), run: onServers }]
        : [];
      items.push({ id: 'add-server', icon: Plug, label: t('menu.addServer'), run: onAddServer });
      return { id: 'webdav', title: t('menu.webdav'), items };
    }
    return {
      id: 'extensions',
      title: t('menu.extensions'),
      items: origin === 'extensions' ? [settingsItem] : [{ id: 'extensions', icon: Puzzle, label: t('menu.openExtensions'), run: onExtensions }],
    };
  });
  if (origin !== 'extensions') groups.push({ id: 'reader', title: '', items: [settingsItem] });

  const items = () => Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []);

  // Opening puts the focus on the first entry; a click outside closes.
  useEffect(() => {
    if (!open) return;
    items()[0]?.focus();
    const onDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [open]);

  const onKeyDown = (event: KeyboardEvent) => {
    const list = items();
    const index = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (event.key === 'ArrowDown') next = (index + 1) % list.length;
    else if (event.key === 'ArrowUp') next = (index - 1 + list.length) % list.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = list.length - 1;
    else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    } else if (event.key === 'Tab') {
      close(false);
      return;
    }
    if (next !== null) {
      event.preventDefault();
      list[next]?.focus();
    }
  };

  return (
    <div className="relative">
      <button
        ref={button}
        type="button"
        className={open ? 'btn btn-ghost btn-sm btn-square btn-active' : 'btn btn-ghost btn-sm btn-square'}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={t('menu.label')}
        title={t('menu.label')}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' && !open) {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <Settings2 className="h-4 w-4" />
      </button>
      {open ? (
        <div
          ref={panel}
          id={menuId}
          role="menu"
          aria-label={t('menu.label')}
          onKeyDown={onKeyDown}
          className="surface-raised manga-menu-in absolute right-0 top-full z-30 mt-2 w-72 p-1.5"
        >
          {groups.map((group, index) => (
            <MenuGroup key={group.id} title={group.title} separated={index > 0}>
              {group.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  disabled={item.id === 'add-folder' && adding}
                  onClick={choose(item.run)}
                  className="flex w-full items-center gap-3 rounded-field px-2.5 py-2 text-left text-sm outline-none motion-safe:transition-colors hover:bg-base-200 focus-visible:bg-base-200 disabled:opacity-50"
                >
                  <item.icon className="h-4 w-4 shrink-0 text-base-content/60" aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate">{item.label}</span>
                  {item.hint ? <span className="shrink-0 text-xs tabular-nums text-base-content/45">{item.hint}</span> : null}
                </button>
              ))}
            </MenuGroup>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function MenuGroup({ title, separated, children }: { title: string; separated: boolean; children: ReactNode }) {
  const id = useId();
  return (
    <div role="group" aria-labelledby={title ? id : undefined} className={separated ? 'mt-1.5 border-t border-base-300/60 pt-1.5' : ''}>
      {title ? <p id={id} className="px-2.5 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-[0.14em] text-base-content/50">{title}</p> : null}
      {children}
    </div>
  );
}
