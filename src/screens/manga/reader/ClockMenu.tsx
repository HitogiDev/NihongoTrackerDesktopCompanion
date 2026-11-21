import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { Check, Flag, Pause, PencilLine, Play, Timer } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { events, ipc } from '../../../lib/ipc';
import { formatClock } from '../../../lib/format';
import type { MangaClock } from '../../../lib/types';
import { useAppStore } from '../../../store/useAppStore';
import { toast } from '../../../store/useToastStore';
import { MangaClockSettings } from '../MangaClockSettings';
import { useMangaSession } from '../useMangaSession';
import { SessionIndicator } from './SessionIndicator';
import { parseClockInput } from './clockTime';

/** The reader's clock as Rust keeps it, from `manga://clock`. */
function useMangaClock(volumeId: string): MangaClock | null {
  const [clock, setClock] = useState<MangaClock | null>(null);
  useEffect(() => {
    let alive = true;
    void ipc.mangaClockState().then((value) => { if (alive) setClock(value); }).catch(() => undefined);
    const off = listen<MangaClock | null>(events.mangaClock, ({ payload }) => setClock(payload));
    return () => {
      alive = false;
      void off.then((stop) => stop());
    };
  }, []);
  return clock?.volume_id === volumeId ? clock : null;
}

/**
 * The session's time, typed by hand: `h:mm:ss`, `mm:ss` or minutes. The
 * clock keeps running meanwhile; what is saved is what was typed. Enter
 * saves, Esc cancels (without closing the menu).
 */
function TimeEditor({ initial, onSave, onCancel }: {
  initial: number;
  onSave: (secs: number) => void | Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useTranslation('mangaReader');
  const [draft, setDraft] = useState(() => formatClock(initial));
  const parsed = parseClockInput(draft);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.select(); }, []);
  const save = () => { if (parsed !== null) void onSave(parsed); };
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-base-content/70" htmlFor="manga-clock-time">{t('clock.editTime')}</label>
      <div className="flex items-center gap-2">
        <input
          ref={input}
          id="manga-clock-time"
          type="text"
          inputMode="numeric"
          autoComplete="off"
          spellCheck={false}
          className={`input input-sm w-32 font-mono tabular-nums ${parsed === null ? 'input-error' : ''}`}
          value={draft}
          aria-invalid={parsed === null}
          aria-describedby="manga-clock-time-hint"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') save();
            if (event.key === 'Escape') {
              event.stopPropagation();
              onCancel();
            }
          }}
        />
        <Button variant="primary" size="sm" disabled={parsed === null} onClick={save}>
          <Check className="h-4 w-4" />
          {t('clock.saveTime')}
        </Button>
        <Button appearance="ghost" size="sm" onClick={onCancel}>{t('clock.cancelTime')}</Button>
      </div>
      <p id="manga-clock-time-hint" className={`text-xs ${parsed === null ? 'text-error' : 'text-base-content/60'}`}>
        {parsed === null ? t('clock.timeInvalid') : t('clock.timeHint')}
      </p>
    </div>
  );
}

/** What the clock is doing, in words. */
function useStatusText(clock: MangaClock | null) {
  const { t } = useTranslation('mangaReader');
  if (!clock) return t('clock.off');
  switch (clock.status) {
    case 'running': return t('clock.running');
    case 'paused': return clock.pause === 'idle' ? t('session.pausedIdle') : t('session.pausedUnfocused');
    case 'stopped': return t('clock.stopped');
    case 'waiting': return clock.starts_in_secs != null ? t('clock.startsIn', { count: clock.starts_in_secs }) : t('clock.waiting');
    case 'off': return t('clock.off');
  }
}

/**
 * The session in the reader's top bar, as a button that opens the clock
 * menu: start or stop the clock, end the session and log it now, and how
 * the clock runs (the same settings as Settings → Manga reader). Before
 * the first tick of reading there is no session yet, only the clock.
 *
 * It follows the session itself, not the reader that holds it: the
 * session's clock changes every second and only this button shows it.
 */
export function ClockMenu({ volumeId, onOpenChange }: {
  volumeId: string;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation('mangaReader');
  const anySession = useMangaSession();
  const session = anySession?.volume_id === volumeId ? anySession : null;
  const settings = useAppStore((s) => s.settings);
  const clock = useMangaClock(volumeId);
  const status = useStatusText(clock);
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const [editingTime, setEditingTime] = useState(false);
  const clockRef = useRef(clock);
  clockRef.current = clock;

  // The clock holds still while its time is edited, so what is saved is
  // what was typed; it runs again however the editing ends (saved,
  // cancelled, menu or window closed) if it was running before.
  useEffect(() => {
    if (!editingTime) return;
    const status = clockRef.current?.status;
    const wasStarted = status === 'running' || status === 'paused';
    if (wasStarted) void ipc.mangaClockAction('stop').catch(() => undefined);
    return () => {
      if (wasStarted) void ipc.mangaClockAction('start').catch(() => undefined);
    };
  }, [editingTime]);

  const toggle = (next: boolean) => {
    setOpen(next);
    if (!next) setEditingTime(false);
    onOpenChange(next);
  };

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) toggle(false);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const running = clock?.status === 'running' || clock?.status === 'paused';
  const act = async (action: 'start' | 'stop' | 'finish') => {
    try {
      await ipc.mangaClockAction(action);
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <div ref={root} className="relative">
      <button
        ref={trigger}
        type="button"
        className="rounded-full focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={t('clock.menu', { status })}
        title={t('clock.menu', { status })}
        onClick={() => toggle(!open)}
      >
        {session ? (
          <SessionIndicator session={session} />
        ) : (
          <span className="hidden h-8 items-center gap-2 rounded-full border border-base-300/70 bg-base-200/70 px-3 text-xs text-base-content/80 sm:inline-flex">
            <Timer className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
            {status}
          </span>
        )}
      </button>
      {open ? (
        <div
          role="dialog"
          aria-label={t('clock.title')}
          className="surface-raised shadow-lg absolute right-0 top-full z-30 mt-2 flex max-h-[min(36rem,calc(100vh-6rem))] w-[22rem] flex-col overflow-hidden text-sm"
          onKeyDown={(event) => {
            if (event.key !== 'Escape') return;
            event.stopPropagation();
            toggle(false);
            trigger.current?.focus();
          }}
        >
          <div className="space-y-3 border-b border-base-300/60 p-3">
            <div className="flex items-center justify-between gap-3">
              <p className="font-semibold">{t('clock.title')}</p>
              {editingTime ? null : (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm gap-1.5 tabular-nums"
                  title={t('clock.editTime')}
                  aria-label={`${t('clock.editTime')}: ${formatClock(session?.elapsed_secs ?? 0)}`}
                  onClick={() => setEditingTime(true)}
                >
                  {formatClock(session?.elapsed_secs ?? 0)}
                  <PencilLine className="h-3.5 w-3.5 opacity-60" aria-hidden="true" />
                </button>
              )}
            </div>
            {editingTime ? (
              <TimeEditor
                initial={session?.elapsed_secs ?? 0}
                onCancel={() => setEditingTime(false)}
                onSave={async (secs) => {
                  try {
                    await ipc.mangaClockSetElapsed(secs);
                    setEditingTime(false);
                  } catch (error) {
                    toast.error(String(error));
                  }
                }}
              />
            ) : null}
            <p className="text-xs text-base-content/70" role="status">{status}</p>
            <div className="flex flex-wrap gap-2">
              {running ? (
                <Button appearance="soft" size="sm" onClick={() => void act('stop')}>
                  <Pause className="h-4 w-4" />
                  {t('clock.stop')}
                </Button>
              ) : (
                <Button variant="primary" size="sm" onClick={() => void act('start')}>
                  <Play className="h-4 w-4" />
                  {t('clock.start')}
                </Button>
              )}
              <Button appearance="outline" size="sm" disabled={!session} title={t('clock.finishHint')} onClick={() => void act('finish')}>
                <Flag className="h-4 w-4" />
                {t('clock.finish')}
              </Button>
            </div>
          </div>
          {settings ? (
            <div className="overflow-y-auto">
              <MangaClockSettings settings={settings} />
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
