import { memo, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { BarChart2, Brain, Save, X } from 'lucide-react';
import { Field } from '../../components/ui/Field';
import { formatClock, numberWithCommas } from '../../lib/format';
import { ipc } from '../../lib/ipc';
import type { HookerContext, HookerTotals, SessionIntelligence, TextLine } from '../../lib/types';
import { useAppStore } from '../../store/useAppStore';
import { SessionClock, SessionSpeed } from './SessionClock';
import type { SessionClockSource } from './useManualTimer';
import { useModalPresence } from './useModalPresence';

function analyzeSession(
  lines: TextLine[],
  mode: SessionIntelligence['detectionMode'],
  manualThreshold: number,
  afkThreshold: number
): SessionIntelligence {
  const japanese = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/gu;
  const chronological = lines
    .map((line) => ({ ...line, timestamp: Date.parse(line.created_at) }))
    .filter((line) => Number.isFinite(line.timestamp))
    .sort((a, b) => a.timestamp - b.timestamp);
  const gaps = chronological.slice(1).map((line, index) =>
    Math.max(0, Math.round((line.timestamp - chronological[index].timestamp) / 1000))
  );
  const baselineGaps = gaps.filter((gap) => gap > 0 && gap < afkThreshold).sort((a, b) => a - b);
  const baseline = baselineGaps.length ? baselineGaps[Math.floor(baselineGaps.length / 2)] : null;
  const threshold = mode === 'manual' ? manualThreshold : mode === 'automatic' && baseline !== null ? Math.max(40, baseline * 2) : null;
  let focusedSeconds = 0;
  let distractedSeconds = 0;
  let afkSeconds = 0;
  let distractionCount = 0;
  let longestDistractionSeconds = 0;
  gaps.forEach((gap) => {
    if (mode === 'off') return;
    if (gap >= afkThreshold) afkSeconds += gap;
    else if (threshold !== null && gap > threshold) {
      const distraction = gap - threshold;
      focusedSeconds += threshold;
      distractedSeconds += distraction;
      distractionCount += 1;
      longestDistractionSeconds = Math.max(longestDistractionSeconds, distraction);
    } else focusedSeconds += gap;
  });
  const firstTimestamp = chronological[0]?.timestamp ?? 0;
  const charactersPerMinute: number[] = [];
  chronological.forEach((line) => {
    const minute = Math.min(1439, Math.floor(Math.max(0, line.timestamp - firstTimestamp) / 60_000));
    while (charactersPerMinute.length <= minute) charactersPerMinute.push(0);
    charactersPerMinute[minute] += (line.text.match(japanese) ?? []).length;
  });
  const attentionSeconds = focusedSeconds + distractedSeconds;
  return {
    detectionMode: mode,
    baselineIntervalSeconds: baseline,
    distractionThresholdSeconds: threshold,
    afkThresholdSeconds: afkThreshold,
    focusedSeconds,
    distractedSeconds,
    afkSeconds,
    distractionCount,
    longestDistractionSeconds,
    focusPercentage: attentionSeconds ? Math.round((focusedSeconds / attentionSeconds) * 100) : 0,
    charactersPerMinute,
  };
}

function durationLabel(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.round(seconds % 60);
  return minutes ? `${minutes}m ${remainder}s` : `${remainder}s`;
}

interface StatsModalProps {
  open: boolean;
  onClose: () => void;
  clock: SessionClockSource;
  contentId: string;
  /** The hooker's title for the work, if it has one. */
  sessionTitle: string | null;
  lines: TextLine[];
  totals: HookerTotals | null;
  context: HookerContext | undefined;
  /** The detected session's cover, when the context has none. */
  autoImage: string | null;
  linesNumber: number;
  charsNumber: number;
  currentSessionChars: number;
  currentSessionLines: number;
  /** After a successful log: reset the clock and reload. */
  onLogged: () => Promise<void>;
}

/** Session and work statistics, and logging the session. The comment, the
 *  logging state and the expanded history entry outlive closing the modal; the
 *  content, and the analysis over every line, exist only while it is open. */
export const StatsModal = memo(function StatsModal(props: StatsModalProps) {
  const { open, onClose } = props;
  const { mounted, shown } = useModalPresence(open);
  const [expandedIntelligenceId, setExpandedIntelligenceId] = useState<string | null>(null);
  const [isLogging, setIsLogging] = useState(false);
  const [logComment, setLogComment] = useState('');
  const [isLogAnimating, setIsLogAnimating] = useState(false);

  return (
    <dialog className={`modal modal-bottom sm:modal-middle ${shown ? 'modal-open' : ''}`}>
      {mounted ? (
        <StatsContent
          {...props}
          expandedIntelligenceId={expandedIntelligenceId}
          setExpandedIntelligenceId={setExpandedIntelligenceId}
          isLogging={isLogging}
          setIsLogging={setIsLogging}
          logComment={logComment}
          setLogComment={setLogComment}
          isLogAnimating={isLogAnimating}
          setIsLogAnimating={setIsLogAnimating}
        />
      ) : null}
      <form method="dialog" className="modal-backdrop">
        <button onClick={onClose}>close</button>
      </form>
    </dialog>
  );
});

function StatsContent({
  onClose, clock, contentId, sessionTitle, lines, totals, context, autoImage,
  linesNumber, charsNumber, currentSessionChars, currentSessionLines, onLogged,
  expandedIntelligenceId, setExpandedIntelligenceId, isLogging, setIsLogging,
  logComment, setLogComment, isLogAnimating, setIsLogAnimating,
}: StatsModalProps & {
  expandedIntelligenceId: string | null;
  setExpandedIntelligenceId: (id: string | null) => void;
  isLogging: boolean;
  setIsLogging: (value: boolean) => void;
  logComment: string;
  setLogComment: (value: string) => void;
  isLogAnimating: boolean;
  setIsLogAnimating: (value: boolean) => void;
}) {
  const { t } = useTranslation(['texthooker', 'common']);
  const online = useAppStore((s) => s.online);
  const intelligenceMode = useAppStore((s) => s.settings?.texthooker_intelligence_mode);
  const manualThreshold = useAppStore((s) => s.settings?.texthooker_intelligence_manual_threshold_seconds);
  const afkThreshold = useAppStore((s) => s.settings?.texthooker_intelligence_afk_threshold_seconds);

  const allTimeLines = Math.max(context?.lines ?? 0, linesNumber);
  const allTimeChars = Math.max(context?.characters ?? 0, charsNumber);
  const progressCharacters = (context?.loggedCharacters ?? 0) + currentSessionChars;
  const progressPercent = context?.totalCharacters
    ? Math.min(100, progressCharacters / context.totalCharacters * 100) : null;
  const history = context?.history;
  const cover = context?.cover ?? autoImage;
  const sessionIntelligence = useMemo(
    () => analyzeSession(
      currentSessionLines > 0 ? lines.slice(-currentSessionLines) : [],
      intelligenceMode ?? 'automatic',
      Math.max(1, manualThreshold ?? 40),
      Math.max(1, afkThreshold ?? 300)
    ),
    [currentSessionLines, lines, afkThreshold, manualThreshold, intelligenceMode]
  );

  async function handleLogSession() {
    if (!totals || currentSessionChars === 0) return;
    setIsLogging(true);
    try {
      await ipc.logTextSession({
        content_id: contentId,
        media_type: totals.mediaType ?? 'vn',
        description: totals.mediaTitle ?? sessionTitle ?? t('sessions.free'),
        comment: logComment.trim(),
        chars: currentSessionChars,
        seconds: clock.current(),
        intelligence: sessionIntelligence,
      });
      setIsLogAnimating(true);
      setLogComment('');
      setTimeout(() => setIsLogAnimating(false), 2000);
      await onLogged();
    } finally {
      setIsLogging(false);
    }
  }

  return (
    <div className="modal-box max-w-4xl">
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-bold text-lg flex items-center gap-2">
          <BarChart2 className="w-5 h-5 text-primary" />
          {t('stats.title')}
        </h3>
        <button
          onClick={onClose}
          className="btn btn-ghost btn-sm btn-circle"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      <div className="flex flex-col md:flex-row gap-6">
        {totals?.mediaTitle && (
          <div className="md:w-1/3 flex-shrink-0">
            <div className="surface-muted p-4 space-y-4">
              {cover && <img src={cover} alt={totals.mediaTitle} className="w-full max-h-80 object-contain rounded-box" />}
              <div>
                <h4 className="font-bold text-lg line-clamp-2">{totals.mediaTitle}</h4>
                <p className="text-sm opacity-70 font-mono">{contentId}</p>
                <div className="flex gap-2 mt-2">
                  <span className="badge badge-primary badge-sm capitalize">
                    {totals.mediaType === 'vn'
                      ? 'visual novel'
                      : totals.mediaType === 'reading'
                        ? 'light novel'
                        : (totals.mediaType ?? t('stats.session'))}
                  </span>
                </div>
              </div>
              <div className="space-y-2 text-sm">
                <div className="flex items-center justify-between gap-2">
                  <span>{t('stats.progress')}</span>
                  {progressPercent !== null && <strong>{progressPercent.toFixed(1)}%</strong>}
                </div>
                {progressPercent !== null ? <>
                  <progress className="progress progress-primary w-full" value={progressPercent} max={100} />
                  <p className="text-xs opacity-70">{t('stats.progressCharacters', { current: numberWithCommas(progressCharacters), total: numberWithCommas(context?.totalCharacters ?? 0) })}</p>
                </> : <p className="text-xs opacity-70">{t('stats.progressUnavailable')}</p>}
                {!online && context?.updatedAt && <p className="text-xs opacity-70">{t('stats.offlineCache')}</p>}
              </div>
            </div>
          </div>
        )}

        <div className="flex-1 space-y-6">
          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
              {t('stats.current')}
            </h4>
            <div className="grid grid-cols-2 gap-3">
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.time')}</div>
                <div className="text-xl font-mono font-bold"><SessionClock clock={clock} /></div>
              </div>
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.speed')}</div>
                <div className="text-xl font-bold">
                  <SessionSpeed clock={clock} chars={currentSessionChars} />
                  <span className="text-xs font-normal opacity-60 ml-1">{t('stats.speedUnit')}</span>
                </div>
              </div>
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.linesRead')}</div>
                <div className="text-xl font-bold">{numberWithCommas(currentSessionLines)}</div>
              </div>
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.characters')}</div>
                <div className="text-xl font-bold">{numberWithCommas(currentSessionChars)}</div>
              </div>
            </div>
            <div className="surface-muted p-3 mt-3 space-y-3">
              <div className="flex items-center gap-2 font-semibold">
                <Brain className="w-4 h-4 text-primary" /> {t('stats.intelligence.title')}
              </div>
              {sessionIntelligence.detectionMode === 'off' ? (
                <p className="text-sm opacity-70">{t('stats.intelligence.disabled')}</p>
              ) : (
                <>
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-sm">
                    <div><span className="opacity-70">{t('stats.intelligence.focus')}</span><div className="font-bold">{currentSessionLines > 1 ? `${sessionIntelligence.focusPercentage}%` : '…'}</div></div>
                    <div><span className="opacity-70">{t('stats.intelligence.focused')}</span><div className="font-bold">{durationLabel(sessionIntelligence.focusedSeconds)}</div></div>
                    <div><span className="opacity-70">{t('stats.intelligence.distracted')}</span><div className="font-bold">{durationLabel(sessionIntelligence.distractedSeconds)}</div></div>
                    <div><span className="opacity-70">{t('stats.intelligence.afk')}</span><div className="font-bold">{durationLabel(sessionIntelligence.afkSeconds)}</div></div>
                  </div>
                  {currentSessionLines < 2 && <p className="text-xs opacity-70">{t('stats.intelligence.waiting')}</p>}
                  {sessionIntelligence.charactersPerMinute.length > 0 && <div className="space-y-1">
                    <div className="flex justify-between gap-2 text-xs opacity-70"><span>{t('stats.intelligence.rate')}</span><span>{Math.max(...sessionIntelligence.charactersPerMinute.slice(-30))}</span></div>
                    <div className="flex items-end gap-1 h-20 border-b border-base-content/20" aria-label={t('stats.intelligence.rate')}>
                      {sessionIntelligence.charactersPerMinute.slice(-30).map((count, index, values) => (
                        <div key={index} className="flex h-full flex-1 items-end justify-center">
                          <div className="w-3 max-w-full bg-primary/70 rounded-t-sm" title={t('stats.intelligence.minute', { minute: Math.max(0, sessionIntelligence.charactersPerMinute.length - 30) + index + 1, count })} style={{ height: `${count / Math.max(1, ...values) * 100}%` }} />
                        </div>
                      ))}
                    </div>
                    <div className="flex justify-between text-xs opacity-60"><span>{t('stats.intelligence.minuteLabel', { minute: Math.max(1, sessionIntelligence.charactersPerMinute.length - 29) })}</span><span>{t('stats.intelligence.minuteLabel', { minute: sessionIntelligence.charactersPerMinute.length })}</span></div>
                  </div>}
                  <div className="text-xs opacity-70">{t('stats.intelligence.distractionCount', { count: sessionIntelligence.distractionCount })}</div>
                </>
              )}
            </div>
          </div>

          <div className="divider"></div>

          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
              {t('stats.total')}
            </h4>
            <div className="grid grid-cols-2 gap-3">
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.totalLines')}</div>
                <div className="text-xl font-bold">{numberWithCommas(allTimeLines)}</div>
              </div>
              <div className="surface-muted p-3">
                <div className="text-xs opacity-70 mb-1">{t('stats.totalCharacters')}</div>
                <div className="text-xl font-bold">{numberWithCommas(allTimeChars)}</div>
              </div>
            </div>
          </div>

          <div className="divider"></div>

          <div>
            <h4 className="text-sm font-semibold uppercase tracking-wider opacity-70 mb-3">
              {t('stats.history')}
            </h4>
            {(history ?? []).length === 0 ? (
              <div className="surface-muted p-3 text-sm opacity-70">
                {t('stats.noHistory')}
              </div>
            ) : (
              <div className="space-y-2 max-h-72 overflow-y-auto pr-1">
                {(history ?? []).map((entry, index) => (
                  <div key={`${entry.loggedAt}-${index}`} className="surface-muted p-3">
                    <button type="button" className="text-xs opacity-70 mb-2 hover:text-primary" onClick={() => setExpandedIntelligenceId(expandedIntelligenceId === entry.id ? null : entry.id)}>
                      {new Date(entry.loggedAt).toLocaleString()}
                      {entry.intelligence && <span className="ml-2 text-primary">{t('stats.intelligence.title')}</span>}
                    </button>
                    <div className="grid grid-cols-2 gap-2 text-sm">
                      <div className="flex justify-between">
                        <span className="opacity-70">{t('stats.characters')}</span>
                        <span className="font-semibold">
                          {numberWithCommas(entry.charactersLogged)}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="opacity-70">{t('stats.speed')}</span>
                        <span className="font-semibold">
                          {numberWithCommas(entry.readingSpeed)} {t('stats.speedUnit')}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="opacity-70">{t('stats.time')}</span>
                        <span className="font-semibold">
                          {formatClock(entry.sessionSeconds)}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="opacity-70">{t('stats.lines')}</span>
                        <span className="font-semibold">
                          {numberWithCommas(entry.linesLogged)}
                        </span>
                      </div>
                    </div>
                    {entry.intelligence && expandedIntelligenceId === entry.id && (
                      <div className="mt-3 pt-3 border-t border-base-content/10 space-y-2">
                        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-sm">
                          <div>{t('stats.intelligence.focus')} <strong>{entry.intelligence.focusPercentage}%</strong></div>
                          <div>{t('stats.intelligence.focused')} <strong>{durationLabel(entry.intelligence.focusedSeconds)}</strong></div>
                          <div>{t('stats.intelligence.distracted')} <strong>{durationLabel(entry.intelligence.distractedSeconds)}</strong></div>
                          <div>{t('stats.intelligence.afk')} <strong>{durationLabel(entry.intelligence.afkSeconds)}</strong></div>
                        </div>
                        <div className="flex items-end gap-1 h-12">
                          {(entry.intelligence.charactersPerMinute.length ? entry.intelligence.charactersPerMinute : [0]).map((count, barIndex, values) => (
                            <div key={barIndex} className="flex-1 bg-primary/70 rounded-t-sm min-w-1" title={`${count} ${t('stats.intelligence.charsPerMinute')}`} style={{ height: `${Math.max(3, (count / Math.max(1, ...values)) * 100)}%` }} />
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>

          <Field label={t('stats.comment')}>
            {(id) => (
              <textarea
                id={id}
                className="textarea w-full"
                rows={3}
                value={logComment}
                onChange={(event) => setLogComment(event.target.value)}
                placeholder={t('stats.commentPlaceholder')}
              />
            )}
          </Field>

          <div className="flex justify-end pt-4 border-t border-base-content/10">
            <button
              onClick={() => void handleLogSession()}
              disabled={isLogging || currentSessionChars === 0}
              className={`btn btn-primary w-full sm:w-auto transition-all ${
                isLogAnimating ? 'btn-success scale-105' : ''
              }`}
            >
              {isLogging ? (
                <span className="loading loading-spinner loading-md"></span>
              ) : isLogAnimating ? (
                <>
                  <span className="text-xl">✓</span>
                  {t('stats.logged')}
                </>
              ) : (
                <>
                  <Save className="w-5 h-5" />
                  {t('stats.logSession')}
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
