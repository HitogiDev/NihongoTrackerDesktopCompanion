import { Fragment } from 'react';
import { useTranslation } from 'react-i18next';
import { MousePointer2 } from 'lucide-react';
import { Modal } from '../../components/ui/Modal';
import type { ShortcutGroup } from './shortcuts';

function Keys({ keys }: { keys: string[][] }) {
  const { t } = useTranslation('mangaReader');
  return (
    <span className="flex flex-wrap items-center justify-end gap-x-1.5 gap-y-1">
      {keys.map((combo, index) => (
        // An alternative wraps with its "or", never leaving it at a line's end.
        <span key={combo.join('+')} className="inline-flex items-center gap-1.5 whitespace-nowrap">
          {index > 0 ? <span className="text-[11px] text-base-content/40">{t('shortcuts.or')}</span> : null}
          <span className="inline-flex items-center gap-0.5">
            {combo.map((key, keyIndex) => (
              <Fragment key={key}>
                {keyIndex > 0 ? <span className="text-[11px] text-base-content/40" aria-hidden="true">+</span> : null}
                <kbd className="kbd kbd-sm min-w-7 font-sans">{t(`shortcuts.key.${key}`, { defaultValue: key })}</kbd>
              </Fragment>
            ))}
          </span>
        </span>
      ))}
    </span>
  );
}

/**
 * Every shortcut of the window, grouped, opened with `?`. The groups come
 * from `shortcuts.ts`; this only lays them out, two columns when there is room.
 */
export function ShortcutsSheet({ open, onClose, groups, note }: {
  open: boolean;
  onClose: () => void;
  groups: ShortcutGroup[];
  /** One line under the title (the reading direction the arrows follow). */
  note?: string;
}) {
  const { t } = useTranslation('mangaReader');
  return (
    <Modal open={open} onClose={onClose} size="xl" title={t('shortcuts.title')} className="manga-shortcuts">
      {note ? <p className="-mt-2 mb-4 text-sm text-base-content/60">{note}</p> : null}
      <div className="columns-1 gap-6 md:columns-2">
        {groups.map((group) => (
          <section key={group.title} className="mb-5 break-inside-avoid" aria-labelledby={`shortcuts-${group.title}`}>
            <h4 id={`shortcuts-${group.title}`} className="mb-1.5 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.16em] text-base-content/55">
              {t(`shortcuts.group.${group.title}`)}
              {group.current ? <span className="badge badge-soft badge-primary badge-xs normal-case tracking-normal">{t('shortcuts.current')}</span> : null}
            </h4>
            <dl className="surface-muted divide-y divide-base-300/60 text-sm">
              {group.items.map((item) => (
                <div key={item.label + (item.mouse ?? '')} className="flex min-h-10 items-center justify-between gap-4 px-3 py-1.5">
                  <dt className="min-w-32 text-base-content/85">{t(`shortcuts.do.${item.label}`)}</dt>
                  <dd className="min-w-0 text-right">
                    {item.mouse ? (
                      <span className="inline-flex items-center gap-1.5 text-xs text-base-content/60">
                        <MousePointer2 className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                        {t(`shortcuts.mouse.${item.mouse}`)}
                      </span>
                    ) : <Keys keys={item.keys} />}
                  </dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </Modal>
  );
}
