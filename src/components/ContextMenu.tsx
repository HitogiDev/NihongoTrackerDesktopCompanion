import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { LucideIcon } from 'lucide-react';

export interface ContextMenuItem {
  id: string;
  label: string;
  icon: LucideIcon;
  onSelect: () => void;
  disabled?: boolean;
  /** Its key, on the right ("Ctrl+C"). */
  hint?: string;
}

/** Space kept between the menu and the window's edges. */
const EDGE = 8;

/**
 * The app's own right-click menu, in place of the webview's ("Back / Reload /
 * Inspect", which `desktop.ts` swallows). `onContextMenu` goes on the element;
 * `menu` is rendered next to it. It also opens from the keyboard (the menu
 * key or Shift+F10 on a focused element), under that element.
 */
export function useContextMenu(label: string, items: () => ContextMenuItem[]): {
  onContextMenu: (event: MouseEvent<HTMLElement>) => void;
  menu: ReactNode;
} {
  const [at, setAt] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const returnTo = useRef<HTMLElement | null>(null);
  const onContextMenu = (event: MouseEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    returnTo.current = event.currentTarget;
    // From the keyboard there is no pointer: under the element.
    const fromKeyboard = event.clientX === 0 && event.clientY === 0;
    const box = event.currentTarget.getBoundingClientRect();
    setAt({ x: fromKeyboard ? box.left + 8 : event.clientX, y: fromKeyboard ? box.bottom : event.clientY, items: items() });
  };
  const close = useCallback((refocus: boolean) => {
    setAt(null);
    if (refocus) returnTo.current?.focus();
  }, []);
  const menu = at ? createPortal(<ContextMenu label={label} x={at.x} y={at.y} items={at.items} onClose={close} />, document.body) : null;
  return { onContextMenu, menu };
}

export function ContextMenu({ label, x, y, items, onClose }: { label: string; x: number; y: number; items: ContextMenuItem[]; onClose: (refocus: boolean) => void }) {
  const panel = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ left: x, top: y });
  const buttons = () => Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []);

  // Inside the window: it opens up or to the left where there is no room.
  useLayoutEffect(() => {
    const box = panel.current?.getBoundingClientRect();
    if (!box) return;
    const left = x + box.width + EDGE > window.innerWidth ? Math.max(EDGE, x - box.width) : x;
    const top = y + box.height + EDGE > window.innerHeight ? Math.max(EDGE, y - box.height) : y;
    setPlace({ left, top });
    buttons()[0]?.focus();
  }, [x, y]);

  // A press outside, scrolling, another window or a resize close it.
  useEffect(() => {
    const onDown = (event: PointerEvent) => {
      if (!panel.current?.contains(event.target as Node)) onClose(false);
    };
    const onAway = () => onClose(false);
    document.addEventListener('pointerdown', onDown, true);
    window.addEventListener('blur', onAway);
    window.addEventListener('resize', onAway);
    document.addEventListener('scroll', onAway, true);
    return () => {
      document.removeEventListener('pointerdown', onDown, true);
      window.removeEventListener('blur', onAway);
      window.removeEventListener('resize', onAway);
      document.removeEventListener('scroll', onAway, true);
    };
  }, [onClose]);

  const onKeyDown = (event: KeyboardEvent) => {
    const list = buttons();
    const index = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (event.key === 'ArrowDown') next = (index + 1) % list.length;
    else if (event.key === 'ArrowUp') next = (index - 1 + list.length) % list.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = list.length - 1;
    else if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault();
      event.stopPropagation();
      onClose(true);
      return;
    }
    if (next !== null) {
      event.preventDefault();
      list[next]?.focus();
    }
  };

  return (
    <div
      ref={panel}
      role="menu"
      aria-label={label}
      onKeyDown={onKeyDown}
      onContextMenu={(event) => event.preventDefault()}
      className="surface-raised manga-menu-in fixed z-50 min-w-56 p-1.5"
      style={{ left: place.left, top: place.top }}
    >
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="menuitem"
          tabIndex={-1}
          disabled={item.disabled}
          onClick={() => {
            onClose(false);
            item.onSelect();
          }}
          className="flex w-full items-center gap-3 rounded-field px-2.5 py-2 text-left text-sm outline-none motion-safe:transition-colors hover:bg-base-200 focus-visible:bg-base-200 disabled:pointer-events-none disabled:opacity-45"
        >
          <item.icon className="h-4 w-4 shrink-0 text-base-content/60" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate">{item.label}</span>
          {item.hint ? <span className="shrink-0 pl-4 text-xs text-base-content/45">{item.hint}</span> : null}
        </button>
      ))}
    </div>
  );
}
