import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { CloudOff, Crown, Globe2, Handshake, Trophy, UserRoundCheck } from 'lucide-react';
import { EmptyState } from '../components/ui/Empty';
import DropdownSelect from '../components/ui/DropdownSelect';
import { Spinner } from '../components/ui/Spinner';
import { numberWithCommas } from '../lib/format';
import { ipc } from '../lib/ipc';
import type { RankingRow } from '../lib/types';
import { useAppStore } from '../store/useAppStore';

const periods = ['all-time', 'today', 'week', 'month', 'year'] as const;
const scopes = ['userXp', 'readingXp', 'listeningXp'] as const;
const audiences = ['all', 'following', 'mutual'] as const;
const media = ['anime', 'manga', 'light-novel', 'vn', 'game', 'video', 'movie', 'tv show', 'book', 'audio'] as const;
type Metric = 'xp' | 'hours' | 'chars';
type MediumMetric = 'xp' | 'time' | 'episodes' | 'pages' | 'chars';
const mediumMetrics: Record<(typeof media)[number], MediumMetric[]> = {
  anime: ['xp', 'time', 'episodes'], manga: ['xp', 'time', 'pages', 'chars'],
  'light-novel': ['xp', 'time', 'chars'], vn: ['xp', 'time', 'chars'],
  game: ['xp', 'time', 'chars'], video: ['xp', 'time'], movie: ['xp', 'time'],
  'tv show': ['xp', 'time'], book: ['xp', 'time', 'pages', 'chars'], audio: ['xp', 'time'],
};

function frameStyle(frame?: string) {
  const colors: Record<string, string> = {
    bronze: 'linear-gradient(135deg,#e0a878,#cd7f32 45%,#7d4a1a)',
    silver: 'linear-gradient(135deg,#f2f2f2,#c0c0c0 45%,#7a7a7a)',
    gold: 'linear-gradient(135deg,#fff0a6,#ffd700 45%,#b8860b)',
    sakura: 'linear-gradient(135deg,#ffe3ef,#f79ac0 50%,#d94f8a)',
    neon: 'linear-gradient(135deg,#00f5ff,#bd00ff 50%,#ff2bd6)',
    rainbow: 'linear-gradient(135deg,#ff4d4d,#ffe66d,#48dbfb,#a55eea)',
    aura: 'conic-gradient(#ff5d5d,#ffcc4d,#50e3c2,#5d8bff,#c75dff,#ff5d5d)',
  };
  return { background: frame ? colors[frame] : undefined };
}

function nameStyle(customization?: RankingRow['customization']) {
  const first = customization?.nameColor1 || 'var(--color-primary)';
  const second = customization?.nameColor2 || 'var(--color-secondary)';
  if (customization?.nameEffect === 'gradient' || customization?.nameEffect === 'shimmer')
    return { backgroundImage: `linear-gradient(90deg, ${first}, ${second}, ${first})`, backgroundClip: 'text', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' };
  if (customization?.nameEffect === 'glow')
    return { color: first, textShadow: `0 0 7px ${first}, 0 0 17px ${second}` };
  return undefined;
}

function Avatar({ row, podium = false, first = false, onOpen }: { row: RankingRow; podium?: boolean; first?: boolean; onOpen: () => void }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [row.avatar]);
  const size = first ? 'h-16 w-16 sm:h-20 sm:w-20' : podium ? 'h-12 w-12 sm:h-16 sm:w-16' : 'h-10 w-10 sm:h-12 sm:w-12';
  return <button type="button" onClick={onOpen} aria-label={row.username} className={`${size} shrink-0 rounded-full p-0.5 ring-1 ring-base-content/20`} style={frameStyle(row.customization?.avatarFrame)}>
    <span className="grid h-full w-full place-items-center overflow-hidden rounded-full bg-base-100 font-bold">
      {row.avatar && !failed ? <img src={row.avatar} alt="" onError={() => setFailed(true)} className="h-full w-full object-cover" /> : row.username.slice(0, 2).toUpperCase()}
    </span>
  </button>;
}

function UserName({ row, onOpen }: { row: RankingRow; onOpen: () => void }) {
  const { t: tCommon } = useTranslation('common');
  const badge = row.patreon;
  const tier = badge?.tier;
  const color = badge?.badgeColor?.trim();
  const customColorsAllowed = tier === 'consumer' || tier === 'donator';
  const customColor = customColorsAllowed && color && !['primary', 'secondary', 'rainbow'].includes(color);
  const badgeClass = !customColorsAllowed || !color || color === 'primary'
    ? 'badge-primary' : color === 'secondary' ? 'badge-secondary' : color === 'rainbow' ? 'badge-rainbow' : '';
  const textColor = badge?.badgeTextColor && !['primary-content', 'secondary-content'].includes(badge.badgeTextColor)
    ? badge.badgeTextColor : undefined;
  const badgeStyle = customColor
    ? { backgroundColor: color, color: textColor || '#ffffff', border: 'none' }
    : color === 'rainbow' && customColorsAllowed
      ? { color: textColor, border: 'none' }
      : { color: textColor };
  const tierLabel = tier === 'consumer' || tier === 'donator' || tier === 'enthusiast'
    ? tCommon(`patreonTiers.${tier}`) : tier;
  return <div className="flex min-w-0 max-w-full items-center justify-center gap-1.5">
    <button type="button" className="min-w-0 truncate font-bold hover:underline" style={nameStyle(row.customization)} onClick={onOpen}>{row.username}</button>
    {badge?.isActive && !badge.hideBadge && tier ? <span className={`badge badge-sm inline-flex shrink-0 items-center justify-center gap-1 align-middle max-w-[8rem] overflow-hidden text-ellipsis whitespace-nowrap ${badgeClass}`} style={badgeStyle}>
      <svg xmlns="http://www.w3.org/2000/svg" className="h-3 w-3 shrink-0" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true"><path fillRule="evenodd" d="M3.172 5.172a4 4 0 015.656 0L10 6.343l1.172-1.171a4 4 0 115.656 5.656L10 17.657l-6.828-6.829a4 4 0 010-5.656z" clipRule="evenodd" /></svg>
      <span className="max-w-[5.5rem] overflow-hidden text-ellipsis whitespace-nowrap font-bold">{badge.customBadgeText?.trim() || tierLabel}</span>
    </span> : null}
  </div>;
}

function PodiumUser({ row, place, value, unit, onOpen }: { row: RankingRow; place: 1 | 2 | 3; value: number; unit: string; onOpen: () => void }) {
  const badge = { 1: 'bg-warning text-warning-content', 2: 'bg-base-content text-base-100', 3: 'bg-accent text-accent-content' }[place];
  return <div className="flex min-w-0 flex-col items-center gap-1 text-center">
    <div className="relative mb-2">{place === 1 ? <Crown className="absolute -top-6 left-1/2 h-6 w-6 -translate-x-1/2 text-warning" /> : null}<Avatar row={row} podium first={place === 1} onOpen={onOpen} /></div>
    <span className={`badge badge-sm font-bold ${badge}`}>{['1st', '2nd', '3rd'][place - 1]}</span>
    <UserName row={row} onOpen={onOpen} />
    <span className="text-sm text-base-content/70">Lv.{row.stats?.userLevel ?? 1}</span>
    <span className={`text-lg font-bold tabular-nums ${place === 1 ? 'text-warning' : ''}`}>{numberWithCommas(value)}</span>
    <span className="text-xs text-base-content/60">{unit}</span>
  </div>;
}

export default function RankingScreen() {
  const { t } = useTranslation('ranking');
  const online = useAppStore((s) => s.online);
  const username = useAppStore((s) => s.settings?.username);
  const profileUrl = useAppStore((s) => s.settings?.api_base_url ?? 'https://nihongotracker.app');
  const [audience, setAudience] = useState<(typeof audiences)[number]>('all');
  const [mode, setMode] = useState<'global' | 'medium'>('global');
  const [scope, setScope] = useState<(typeof scopes)[number]>('userXp');
  const [metric, setMetric] = useState<Metric>('xp');
  const [mediumType, setMediumType] = useState<(typeof media)[number]>('anime');
  const [mediumMetric, setMediumMetric] = useState<MediumMetric>('xp');
  const [timeFilter, setTimeFilter] = useState<(typeof periods)[number]>('month');
  const [limit, setLimit] = useState(25);
  const byMedium = mode === 'medium';
  const filter = metric === 'chars' ? 'userChars' : metric === 'hours' ? scope.replace('Xp', 'Hours') : scope;
  const unit = byMedium ? mediumMetric === 'time' ? t('units.hours') : t(`metrics.${mediumMetric}`) : metric === 'hours' ? t('units.hours') : metric === 'chars' ? t('metrics.chars') : 'XP';
  const label = byMedium ? `${t(`medium.${mediumType}`)} · ${t(`metrics.${mediumMetric}`)}` : `${t(`scope.${scope}`)} · ${t(`metrics.${metric}`)}`;
  const getValue = (row: RankingRow) => byMedium ? mediumMetric === 'time' ? row.hours ?? 0 : row[mediumMetric] ?? 0 : row.stats?.[filter] ?? 0;

  const { data, isFetching, isError } = useQuery({
    queryKey: ['ranking', audience, mode, scope, metric, mediumType, mediumMetric, timeFilter, limit],
    queryFn: () => ipc.ranking({ filter, timeFilter, limit, audience, mediumType: byMedium ? mediumType : undefined, mediumMetric: byMedium ? mediumMetric : undefined }),
    enabled: online === true,
    retry: false,
  });
  const rows = data ?? [];
  const openProfile = (row: RankingRow) => void openUrl(`${profileUrl.replace(/\/$/, '')}/user/${encodeURIComponent(row.username)}`).catch(() => undefined);

  return <div className="min-h-full bg-base-200 px-4 py-8 sm:px-8"><div className="mx-auto max-w-5xl">
    <header className="mb-8 text-center"><div className="mb-2 flex items-center justify-center gap-3"><Trophy className="h-9 w-9 text-warning" /><h1 className="text-3xl font-bold">{t('title')}</h1></div><p className="text-base-content/70">{t('subtitle')}</p></header>
    <div className="mb-4 flex justify-center"><div className="join" role="group" aria-label={t('audience.label')}>
      {audiences.map((value) => { const Icon = value === 'all' ? Globe2 : value === 'following' ? UserRoundCheck : Handshake; return <button key={value} type="button" className={`join-item btn btn-sm ${audience === value ? 'btn-primary' : 'btn-outline'}`} onClick={() => { setAudience(value); setLimit(25); }}><Icon className="h-4 w-4" />{t(`audience.${value}`)}</button>; })}
    </div></div>
    <div className="mb-8 flex flex-wrap items-center justify-center gap-3">
      <div className="join" role="group" aria-label={t('modes.label')}><button type="button" className={`join-item btn btn-sm ${!byMedium ? 'btn-primary' : 'btn-outline'}`} onClick={() => { setMode('global'); setLimit(25); }}>{t('modes.global')}</button><button type="button" className={`join-item btn btn-sm ${byMedium ? 'btn-primary' : 'btn-outline'}`} onClick={() => { setMode('medium'); setLimit(25); }}>{t('modes.medium')}</button></div>
      <DropdownSelect className="select select-sm w-auto" aria-label={t('time.label')} value={timeFilter} onChange={(event) => { setTimeFilter(event.target.value as typeof timeFilter); setLimit(25); }}>{periods.map((value) => <option key={value} value={value}>{t(`time.${value}`)}</option>)}</DropdownSelect>
      {byMedium ? <div className="flex gap-2"><DropdownSelect className="select select-sm w-auto" aria-label={t('medium.label')} value={mediumType} onChange={(event) => { const next = event.target.value as typeof mediumType; setMediumType(next); setMediumMetric(mediumMetrics[next][0]); setLimit(25); }}>{media.map((value) => <option key={value} value={value}>{t(`medium.${value}`)}</option>)}</DropdownSelect><DropdownSelect className="select select-sm w-auto" aria-label={t('metrics.label')} value={mediumMetric} onChange={(event) => { setMediumMetric(event.target.value as MediumMetric); setLimit(25); }}>{mediumMetrics[mediumType].map((value) => <option key={value} value={value}>{t(`metrics.${value}`)}</option>)}</DropdownSelect></div> : <div className="flex gap-2"><DropdownSelect className="select select-sm w-auto" aria-label={t('scope.label')} value={scope} onChange={(event) => { const next = event.target.value as typeof scope; setScope(next); if (next === 'listeningXp' && metric === 'chars') setMetric('xp'); setLimit(25); }}>{scopes.map((value) => <option key={value} value={value}>{t(`scope.${value}`)}</option>)}</DropdownSelect><DropdownSelect className="select select-sm w-auto" aria-label={t('metrics.label')} value={metric} onChange={(event) => { setMetric(event.target.value as Metric); setLimit(25); }}>{(['xp', 'hours', ...(scope === 'listeningXp' ? [] : ['chars'])] as Metric[]).map((value) => <option key={value} value={value}>{t(`metrics.${value}`)}</option>)}</DropdownSelect></div>}
    </div>
    {online === false ? <div className="surface"><EmptyState icon={CloudOff} title={t('offline')} /></div> : isError ? <div className="surface"><EmptyState icon={CloudOff} title={t('loadFailed')} /></div> : rows.length === 0 && !isFetching ? <div className="surface"><EmptyState icon={Trophy} title={audience === 'all' ? t('empty') : t('audience.empty')} /></div> : <div className="card surface overflow-hidden"><div className="card-body p-0">
      {rows.length >= 3 ? <div className="bg-primary/15 px-3 pb-7 pt-10 sm:px-8"><div className="mx-auto grid max-w-2xl grid-cols-3 items-end gap-2 sm:gap-4">{([1, 0, 2] as const).map((index) => <PodiumUser key={rows[index].username} row={rows[index]} place={(index + 1) as 1 | 2 | 3} value={getValue(rows[index])} unit={unit} onOpen={() => openProfile(rows[index])} />)}</div></div> : null}
      <div className="overflow-x-auto"><table className="table w-full"><thead><tr><th className="w-14 text-center">{t('table.rank')}</th><th>{t('table.user')}</th><th className="hidden text-center sm:table-cell">{t('table.level')}</th><th className="text-right">{t('columnHeader', { filter: label, unit })}</th></tr></thead><tbody>{rows.map((row, index) => { if (rows.length >= 3 && index < 3) return null; const isMe = row.username === username; return <tr key={row.username} className={isMe ? 'bg-primary/10' : ''}><td className="text-center font-bold text-base-content/70 tabular-nums">{index + 1}</td><td><div className="flex min-w-0 items-center gap-2 sm:gap-3"><Avatar row={row} onOpen={() => openProfile(row)} /><UserName row={row} onOpen={() => openProfile(row)} /></div></td><td className="hidden text-center sm:table-cell"><span className="badge badge-outline">Lv.{row.stats?.userLevel ?? 1}</span></td><td className="text-right"><div className="font-bold tabular-nums">{numberWithCommas(getValue(row))}</div><div className="text-xs text-base-content/60">{unit}</div></td></tr>; })}</tbody></table></div>
      {rows.length >= limit ? <div className="border-t border-base-300 p-5 text-center"><button type="button" className="btn btn-primary btn-sm" onClick={() => setLimit((value) => value + 25)}>{t('loadMore')}</button></div> : null}
    </div></div>}
    {isFetching ? <div className="mt-3 flex justify-center"><Spinner size="sm" /></div> : null}
  </div></div>;
}
