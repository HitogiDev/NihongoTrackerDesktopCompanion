import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { ClipboardPaste, Copy, Scissors, TextSelect } from 'lucide-react';
import { ipc } from '../lib/ipc';
import { shortcutLabel } from '../lib/desktop';
import { ContextMenu, type ContextMenuItem } from './ContextMenu';

type Field = HTMLInputElement | HTMLTextAreaElement;

/** Inputs whose text can be selected (a checkbox or a range has none). */
const TEXT_INPUT = 'input:not([type]), input[type="text"], input[type="search"], input[type="url"], input[type="tel"], input[type="email"], input[type="password"], input[type="number"], textarea';

function selectionOf(field: Field): [number, number] | null {
  try {
    return field.selectionStart === null || field.selectionEnd === null ? null : [field.selectionStart, field.selectionEnd];
  } catch {
    // A number input has no selection to ask about.
    return null;
  }
}

/**
 * Right click on text: Cut, Copy, Paste and Select all, the app's own. The
 * webview's menu (what fields used to keep) carries "Inspect" and browser
 * entries, so it never shows (`desktop.ts`). Paste reads the clipboard
 * through Rust (`clipboard_text`): the webview would ask for permission.
 * Editing goes through `execCommand`, so fields fire their input events and
 * Ctrl+Z still undoes it. Mounted once, for every window.
 */
export function TextMenu() {
  const { t } = useTranslation('common');
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);

  useEffect(() => {
    const onMenu = (event: MouseEvent) => {
      if (import.meta.env.DEV && event.shiftKey) return;
      const target = event.target instanceof Element ? event.target : null;
      const field = target?.closest<Field>(TEXT_INPUT) ?? null;
      const editable = field ? null : target?.closest<HTMLElement>('[contenteditable=""], [contenteditable="true"]') ?? null;
      const readable = field || editable ? null : target?.closest<HTMLElement>('.selectable') ?? null;
      if (!field && !editable && !readable) return;

      const range = field ? selectionOf(field) : null;
      const selected = field
        ? (range ? field.value.slice(range[0], range[1]) : '')
        : window.getSelection()?.toString() ?? '';
      const secret = field instanceof HTMLInputElement && field.type === 'password';
      const writable = field ? !field.readOnly && !field.disabled : Boolean(editable);
      const back = () => {
        const element = field ?? editable;
        element?.focus();
        if (field && range) field.setSelectionRange(range[0], range[1]);
      };
      const copy = () => navigator.clipboard.writeText(selected).catch(() => undefined);

      const items: ContextMenuItem[] = [];
      if (field || editable) {
        items.push({
          id: 'cut',
          label: t('textMenu.cut'),
          icon: Scissors,
          hint: shortcutLabel('X'),
          disabled: !writable || !selected || secret,
          onSelect: () => {
            void copy();
            back();
            document.execCommand('delete');
          },
        });
      }
      items.push({ id: 'copy', label: t('textMenu.copy'), icon: Copy, hint: shortcutLabel('C'), disabled: !selected || secret, onSelect: () => void copy() });
      if (field || editable) {
        items.push({
          id: 'paste',
          label: t('textMenu.paste'),
          icon: ClipboardPaste,
          hint: shortcutLabel('V'),
          disabled: !writable,
          onSelect: () => {
            void ipc.clipboardText().then((text) => {
              back();
              if (text) document.execCommand('insertText', false, text);
            }).catch(() => undefined);
          },
        });
      }
      items.push({
        id: 'all',
        label: t('textMenu.selectAll'),
        icon: TextSelect,
        hint: shortcutLabel('A'),
        onSelect: () => {
          if (field) {
            field.focus();
            field.select();
            return;
          }
          const node = editable ?? readable;
          if (!node) return;
          editable?.focus();
          const selection = window.getSelection();
          const all = document.createRange();
          all.selectNodeContents(node);
          selection?.removeAllRanges();
          selection?.addRange(all);
        },
      });
      event.preventDefault();
      setMenu({ x: event.clientX, y: event.clientY, items });
    };
    window.addEventListener('contextmenu', onMenu);
    return () => window.removeEventListener('contextmenu', onMenu);
  }, [t]);

  const close = useCallback(() => setMenu(null), []);
  return menu ? createPortal(<ContextMenu label={t('textMenu.label')} x={menu.x} y={menu.y} items={menu.items} onClose={close} />, document.body) : null;
}
