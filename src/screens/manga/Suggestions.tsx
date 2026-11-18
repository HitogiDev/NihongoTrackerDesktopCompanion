import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { ArrowRight, Check, Search, Sparkles, X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { EmptyState } from '../../components/ui/Empty';
import { mangaUrl } from '../../lib/manga';
import type { MangaLinkSuggestion, MangaSeries } from '../../lib/types';
import { Cover, MANGA } from './Cover';
import { confirmSuggestions, dismissSuggestion, scorePercent } from './useSuggestions';

/** The NihongoTracker cover of a suggestion; blurred when the media is adult. */
function MediaImage({ suggestion, className }: { suggestion: MangaLinkSuggestion; className: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <div className={`overflow-hidden rounded-field bg-base-200 ring-1 ring-base-content/10 ${className}`}>
      {suggestion.media_image && !failed ? (
        <img
          src={suggestion.media_image}
          alt=""
          draggable={false}
          onError={() => setFailed(true)}
          className={suggestion.is_adult ? 'h-full w-full scale-110 object-cover blur-md' : 'h-full w-full object-cover'}
        />
      ) : (
        <div className={`grid h-full w-full place-items-center ${MANGA.color}`}>
          <Sparkles className="h-5 w-5 opacity-50" aria-hidden="true" />
        </div>
      )}
    </div>
  );
}

function Score({ score }: { score: number }) {
  const { t } = useTranslation('manga');
  const percent = scorePercent(score);
  return (
    <span className={percent >= 90 ? 'badge badge-soft badge-success badge-sm tabular-nums' : 'badge badge-soft badge-warning badge-sm tabular-nums'} title={t('suggest.scoreHint')}>
      {t('suggest.score', { percent })}
    </span>
  );
}

/** "Is this it?": the suggestion of the series page, to confirm, replace or dismiss. */
export function SuggestionCard({ suggestion, onChoose }: { suggestion: MangaLinkSuggestion; onChoose: () => void }) {
  const { t } = useTranslation('manga');
  const [busy, setBusy] = useState<'confirm' | 'dismiss' | null>(null);
  const run = async (kind: 'confirm' | 'dismiss') => {
    setBusy(kind);
    try {
      if (kind === 'confirm') await confirmSuggestions([suggestion]);
      else await dismissSuggestion(suggestion);
    } finally {
      setBusy(null);
    }
  };
  return (
    <section aria-labelledby="suggestion-title" className="surface manga-fade-in flex flex-col gap-4 p-4 sm:flex-row sm:items-center">
      <MediaImage suggestion={suggestion} className="aspect-[2/3] w-16 shrink-0" />
      <div className="min-w-0 flex-1">
        <p id="suggestion-title" className={`flex items-center gap-1.5 text-xs font-semibold uppercase tracking-[0.14em] ${MANGA.color}`}>
          <Sparkles className="h-3.5 w-3.5" aria-hidden="true" />
          {t('suggest.isThisIt')}
        </p>
        <p className="mt-1 flex min-w-0 flex-wrap items-center gap-2">
          <span className="truncate text-base font-semibold" title={suggestion.media_title}>{suggestion.media_title}</span>
          <Score score={suggestion.score} />
          {suggestion.is_adult ? <span className="badge badge-soft badge-error badge-sm">{t('common:media.adult')}</span> : null}
        </p>
        {suggestion.media_subtitle ? <p className="truncate text-sm text-base-content/60">{suggestion.media_subtitle}</p> : null}
        <p className="mt-0.5 text-xs text-base-content/50">{t('suggest.why', { query: suggestion.query })}</p>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-2">
        <Button variant="primary" size="sm" loading={busy === 'confirm'} disabled={busy !== null} onClick={() => void run('confirm')}>
          {busy === 'confirm' ? null : <Check className="h-4 w-4" />}
          {t('suggest.confirm')}
        </Button>
        <Button appearance="outline" size="sm" disabled={busy !== null} onClick={onChoose}>
          <Search className="h-4 w-4" />
          {t('suggest.choose')}
        </Button>
        <Button appearance="ghost" size="sm" loading={busy === 'dismiss'} disabled={busy !== null} onClick={() => void run('dismiss')}>
          {busy === 'dismiss' ? null : <X className="h-4 w-4" />}
          {t('suggest.dismiss')}
        </Button>
      </div>
    </section>
  );
}

/** A quiet mark on a grid cover: this series has a suggestion waiting. */
export function SuggestionBadge({ className = '' }: { className?: string }) {
  const { t } = useTranslation('manga');
  return (
    <span role="img" aria-label={t('suggest.badge')} title={t('suggest.badge')} className={`grid h-6 w-6 place-items-center rounded-full bg-base-100/90 shadow-sm backdrop-blur-sm ${MANGA.color} ${className}`}>
      <Sparkles className="h-3.5 w-3.5" aria-hidden="true" />
    </span>
  );
}

/**
 * Every suggestion waiting, side by side with its series: each one visible
 * before it is confirmed, several confirmed in one click.
 */
export function SuggestionReview({ series, suggestions }: { series: MangaSeries[]; suggestions: Map<string, MangaLinkSuggestion> }) {
  const { t } = useTranslation('manga');
  const rows = useMemo(
    () => series.flatMap((item) => {
      const suggestion = suggestions.get(item.id);
      return suggestion && !item.media_id ? [{ series: item, suggestion }] : [];
    }),
    [series, suggestions],
  );
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  // What is still on the list: a series linked meanwhile drops out.
  const chosen = rows.filter((row) => picked.has(row.series.id));
  const all = rows.length > 0 && chosen.length === rows.length;
  const toggle = (id: string) => setPicked((current) => {
    const next = new Set(current);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  });
  const confirm = async (list: MangaLinkSuggestion[]) => {
    setConfirming(true);
    try {
      await confirmSuggestions(list);
      setPicked(new Set());
    } finally {
      setConfirming(false);
    }
  };

  if (rows.length === 0) {
    return (
      <div className="surface">
        <EmptyState icon={Sparkles} title={t('suggest.review.none')} description={t('suggest.review.noneBody')} />
      </div>
    );
  }
  return (
    <section className="space-y-3" aria-label={t('suggest.review.title')}>
      <div className="surface flex flex-wrap items-center gap-3 px-4 py-2.5">
        <label className="flex cursor-pointer items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="checkbox checkbox-sm"
            checked={all}
            ref={(element) => { if (element) element.indeterminate = chosen.length > 0 && !all; }}
            onChange={() => setPicked(all ? new Set() : new Set(rows.map((row) => row.series.id)))}
          />
          {t('suggest.review.selectAll')}
        </label>
        <p className="min-w-0 flex-1 text-xs text-base-content/55">{t('suggest.review.body')}</p>
        <Button variant="primary" size="sm" loading={confirming} disabled={chosen.length === 0} onClick={() => void confirm(chosen.map((row) => row.suggestion))}>
          {confirming ? null : <Check className="h-4 w-4" />}
          {t('suggest.review.confirmSelected', { count: chosen.length })}
        </Button>
      </div>
      <ul className="surface divide-y divide-base-300/60 overflow-hidden">
        {rows.map(({ series: item, suggestion }) => (
          <li key={item.id} className={picked.has(item.id) ? 'flex items-center gap-4 bg-primary/5 px-4 py-3' : 'flex items-center gap-4 px-4 py-3'}>
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={picked.has(item.id)}
              onChange={() => toggle(item.id)}
              aria-label={t('suggest.review.pick', { title: item.title, media: suggestion.media_title })}
            />
            <Link to={`/manga/${item.id}`} className="flex min-w-0 flex-1 items-center gap-3 rounded-field focus-visible:outline-offset-2" title={item.title}>
              <div className="aspect-[2/3] w-11 shrink-0 overflow-hidden rounded-field ring-1 ring-base-content/5">
                <Cover src={mangaUrl.cover(item.id)} />
              </div>
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium">{item.title}</span>
                <span className="block truncate text-xs text-base-content/50">{t('suggest.why', { query: suggestion.query })}</span>
              </span>
            </Link>
            <ArrowRight className="hidden h-4 w-4 shrink-0 text-base-content/30 md:block" aria-hidden="true" />
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <MediaImage suggestion={suggestion} className="aspect-[2/3] w-11 shrink-0" />
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium" title={suggestion.media_title}>{suggestion.media_title}</span>
                <span className="flex items-center gap-1.5">
                  <Score score={suggestion.score} />
                  {suggestion.media_subtitle ? <span className="truncate text-xs text-base-content/50">{suggestion.media_subtitle}</span> : null}
                </span>
              </span>
            </div>
            <div className="flex shrink-0 items-center">
              <Button appearance="ghost" size="sm" shape="square" disabled={confirming} aria-label={t('suggest.review.confirmOne', { title: item.title })} title={t('suggest.review.confirmOne', { title: item.title })} onClick={() => void confirm([suggestion])}>
                <Check className="h-4 w-4" />
              </Button>
              <Button appearance="ghost" size="sm" shape="square" disabled={confirming} aria-label={t('suggest.review.dismissOne', { title: item.title })} title={t('suggest.review.dismissOne', { title: item.title })} onClick={() => void dismissSuggestion(suggestion)}>
                <X className="h-4 w-4" />
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
