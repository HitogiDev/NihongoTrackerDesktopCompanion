import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { CloudOff, Crown, Trophy } from 'lucide-react';
import { EmptyState } from '../components/ui/Empty';
import { Page } from '../components/ui/Page';
import { Spinner } from '../components/ui/Spinner';
import { numberWithCommas } from '../lib/format';
import { ipc } from '../lib/ipc';
import { useAppStore } from '../store/useAppStore';

const FILTERS = ['userXp', 'readingXp', 'listeningXp'] as const;
const PERIODS = ['all-time', 'today', 'month', 'year'] as const;

export default function RankingScreen() {
  const { t } = useTranslation('ranking');
  const online = useAppStore((s) => s.online);
  const username = useAppStore((s) => s.settings?.username);

  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('userXp');
  const [timeFilter, setTimeFilter] = useState<(typeof PERIODS)[number]>('all-time');
  const [limit, setLimit] = useState(25);

  const { data, isFetching } = useQuery({
    queryKey: ['ranking', filter, timeFilter, limit],
    queryFn: () => ipc.ranking({ filter, timeFilter, limit }),
    enabled: online === true,
    retry: false,
  });

  const rows = data ?? [];

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={isFetching ? <Spinner size="sm" /> : undefined}
    >
      {online === false ? (
        <div className="surface">
          <EmptyState icon={CloudOff} title={t('offline')} />
        </div>
      ) : (
        <>
          <div className="flex flex-wrap gap-2">
            <div className="join">
              {FILTERS.map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`join-item btn btn-sm ${
                    filter === value ? 'btn-primary' : 'btn-ghost'
                  }`}
                  onClick={() => setFilter(value)}
                >
                  {t(`filter.${value}`)}
                </button>
              ))}
            </div>
            <div className="join ml-auto">
              {PERIODS.map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`join-item btn btn-sm ${
                    timeFilter === value ? 'btn-primary' : 'btn-ghost'
                  }`}
                  onClick={() => setTimeFilter(value)}
                >
                  {t(`time.${value}`)}
                </button>
              ))}
            </div>
          </div>

          {rows.length === 0 && !isFetching ? (
            <div className="surface">
              <EmptyState icon={Trophy} title={t('empty')} />
            </div>
          ) : (
            <ul className="space-y-1.5">
              {rows.map((row, index) => {
                const isMe = row.username === username;
                const position = index + 1;
                return (
                  <li
                    key={row.username}
                    className={`flex items-center gap-3 px-3 py-2 rounded-field transition-colors ${
                      isMe ? 'bg-primary/10 border border-primary/30' : 'bg-base-100/60'
                    }`}
                  >
                    <span
                      className={`w-10 shrink-0 text-right font-bold tabular-nums ${
                        position <= 3 ? 'text-warning' : 'text-base-content/50'
                      }`}
                    >
                      {t('position', { position })}
                    </span>
                    {position === 1 ? (
                      <Crown className="w-4 h-4 text-warning shrink-0" />
                    ) : null}
                    <div className="w-8 h-8 rounded-full bg-base-300 overflow-hidden shrink-0">
                      {row.avatar ? (
                        <img src={row.avatar} alt="" className="w-full h-full object-cover" />
                      ) : (
                        <div className="w-full h-full flex items-center justify-center text-xs font-bold text-base-content/60">
                          {row.username.charAt(0).toUpperCase()}
                        </div>
                      )}
                    </div>
                    <span className="min-w-0 flex-1 truncate font-medium">
                      {row.username}
                      {isMe ? (
                        <span className="badge badge-primary badge-xs ml-2">{t('you')}</span>
                      ) : null}
                    </span>
                    <span className="tabular-nums font-semibold shrink-0">
                      {numberWithCommas(row.stats?.[filter] ?? 0)}
                      <span className="text-xs font-normal text-base-content/50 ml-1">XP</span>
                    </span>
                  </li>
                );
              })}
            </ul>
          )}

          {rows.length >= limit ? (
            <button
              type="button"
              className="btn btn-ghost btn-sm btn-block"
              onClick={() => setLimit((value) => value + 25)}
            >
              {t('loadMore')}
            </button>
          ) : null}
        </>
      )}
    </Page>
  );
}
