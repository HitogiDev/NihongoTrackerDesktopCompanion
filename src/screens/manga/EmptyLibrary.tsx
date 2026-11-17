import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { BookOpen, FolderPlus, Puzzle, Server, type LucideIcon } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { MANGA } from './Cover';

function Way({ icon: Icon, title, body, action, soon = false }: {
  icon: LucideIcon;
  title: string;
  body: string;
  action?: ReactNode;
  soon?: boolean;
}) {
  const { t } = useTranslation('manga');
  return (
    <li className={soon ? 'surface-muted flex flex-col p-5' : 'surface flex flex-col p-5'}>
      <div className="flex items-center justify-between gap-3">
        <span className={soon ? 'grid h-10 w-10 place-items-center rounded-field bg-base-300 text-base-content/50' : `grid h-10 w-10 place-items-center rounded-field ${MANGA.bgColor} ${MANGA.color}`}>
          <Icon className="h-5 w-5" aria-hidden="true" />
        </span>
        {soon ? <span className="badge badge-ghost badge-sm">{t('tabs.soon')}</span> : null}
      </div>
      <h3 className={soon ? 'mt-4 font-semibold text-base-content/70' : 'mt-4 font-semibold'}>{title}</h3>
      <p className="mt-1 flex-1 text-sm text-base-content/60">{body}</p>
      {action ? <div className="mt-4">{action}</div> : null}
    </li>
  );
}

/** No library yet: the three ways manga will get here, one of them ready. */
export function EmptyLibrary({ onAdd, adding }: { onAdd: () => void; adding: boolean }) {
  const { t } = useTranslation('manga');
  return (
    <section className="mx-auto max-w-4xl py-8 text-center">
      <span className={`mx-auto grid h-16 w-16 place-items-center rounded-full ${MANGA.bgColor} ${MANGA.color}`}>
        <BookOpen className="h-8 w-8" aria-hidden="true" />
      </span>
      <h2 className="mt-5 text-xl font-bold">{t('empty.title')}</h2>
      <p className="mx-auto mt-2 max-w-lg text-sm text-base-content/60">{t('empty.body')}</p>
      <ul className="mt-8 grid gap-4 text-left sm:grid-cols-3">
        <Way
          icon={FolderPlus}
          title={t('empty.local.title')}
          body={t('empty.local.body')}
          action={
            <Button variant="primary" size="sm" loading={adding} onClick={onAdd}>
              {adding ? null : <FolderPlus className="h-4 w-4" />}
              {t('actions.addFolder')}
            </Button>
          }
        />
        <Way icon={Server} title={t('empty.webdav.title')} body={t('empty.webdav.body')} soon />
        <Way icon={Puzzle} title={t('empty.extensions.title')} body={t('empty.extensions.body')} soon />
      </ul>
      <p className="mx-auto mt-6 max-w-xl text-xs text-base-content/50">{t('empty.tip')}</p>
    </section>
  );
}
