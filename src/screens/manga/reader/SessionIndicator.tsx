import { useTranslation } from 'react-i18next';
import { Clock, Files, Type } from 'lucide-react';
import { formatClock, numberWithCommas } from '../../../lib/format';
import type { MangaSession } from '../../../lib/types';

/**
 * The session in the reader's top bar: whether the clock is running, how long
 * it has run, and what has been counted. Rust keeps all of it; a paused clock
 * says why.
 */
export function SessionIndicator({ session }: { session: MangaSession }) {
  const { t } = useTranslation('mangaReader');
  const status = session.counting
    ? t('session.counting')
    : session.pause === 'idle'
      ? t('session.pausedIdle')
      : t('session.pausedUnfocused');
  const summary = t('session.summary', {
    time: formatClock(session.elapsed_secs),
    pages: session.pages_read,
    chars: numberWithCommas(session.chars_read),
  });
  return (
    <div
      className="hidden h-8 shrink-0 items-center gap-2.5 rounded-full border border-base-300/70 bg-base-200/70 px-3 text-xs tabular-nums text-base-content/80 sm:inline-flex"
      role="status"
      aria-label={`${status} · ${summary}`}
      title={`${status} · ${summary}`}
    >
      <span className={session.counting ? 'h-2 w-2 shrink-0 rounded-full bg-success recording-dot' : 'h-2 w-2 shrink-0 rounded-full bg-warning'} aria-hidden="true" />
      <span className="flex items-center gap-1 font-semibold text-base-content">
        <Clock className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
        {formatClock(session.elapsed_secs)}
      </span>
      <span className="h-3.5 w-px bg-base-content/15" aria-hidden="true" />
      <span className="flex items-center gap-1" title={t('session.pagesHint')}>
        <Files className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
        {t('session.pages', { count: session.pages_read })}
      </span>
      {session.has_text ? (
        <span className="hidden items-center gap-1 md:flex">
          <Type className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
          {t('session.chars', { count: session.chars_read })}
        </span>
      ) : null}
      {!session.counting ? <span className="hidden font-medium text-warning lg:inline">{status}</span> : null}
    </div>
  );
}
