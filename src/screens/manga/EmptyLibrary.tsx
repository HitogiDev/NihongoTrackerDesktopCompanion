import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { BookOpen, FolderPlus, Puzzle, Server, type LucideIcon } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { MANGA } from './Cover';

function Way({ icon: Icon, title, body, action }: {
  icon: LucideIcon;
  title: string;
  body: string;
  action?: ReactNode;
}) {
  return (
    <li className="surface flex flex-col p-5">
      <span className={`grid h-10 w-10 place-items-center rounded-field ${MANGA.bgColor} ${MANGA.color}`}>
        <Icon className="h-5 w-5" aria-hidden="true" />
      </span>
      <h3 className="mt-4 font-semibold">{title}</h3>
      <p className="mt-1 flex-1 text-sm text-base-content/60">{body}</p>
      {action ? <div className="mt-4">{action}</div> : null}
    </li>
  );
}

/** No library yet: the three ways manga gets here. */
export function EmptyLibrary({ onAdd, adding, onAddServer, onExtensions }: { onAdd: () => void; adding: boolean; onAddServer: () => void; onExtensions: () => void }) {
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
        <Way
          icon={Server}
          title={t('empty.webdav.title')}
          body={t('empty.webdav.body')}
          action={
            <Button appearance="outline" size="sm" onClick={onAddServer}>
              <Server className="h-4 w-4" />
              {t('webdav.actions.add')}
            </Button>
          }
        />
        <Way
          icon={Puzzle}
          title={t('empty.extensions.title')}
          body={t('empty.extensions.body')}
          action={
            <Button appearance="outline" size="sm" onClick={onExtensions}>
              <Puzzle className="h-4 w-4" />
              {t('empty.extensions.action')}
            </Button>
          }
        />
      </ul>
      <p className="mx-auto mt-6 max-w-xl text-xs text-base-content/50">{t('empty.tip')}</p>
    </section>
  );
}
