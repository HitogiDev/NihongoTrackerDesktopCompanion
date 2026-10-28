import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { useTranslation } from 'react-i18next';
import { BookOpenText, BookmarkPlus, Minimize2, Maximize2, Pause, Play, Plus, X } from 'lucide-react';
import { events, ipc } from '../lib/ipc';
import { miningError } from '../lib/mining';
import { formatClock } from '../lib/format';
import { useAppStore } from '../store/useAppStore';

export function SessionOverlay() {
  const { t } = useTranslation('overlay');
  const tracker = useAppStore((s) => s.tracker);
  const hooker = useAppStore((s) => s.hooker);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const queryClient = useQueryClient();
  const [feedback, setFeedback] = useState<string | null>(null);
  const [compact, setCompact] = useState(false);
  const contentId = hooker?.content_id ?? 'local';
  const latest = useQuery({ queryKey: ['overlay-line', contentId], queryFn: () => ipc.hookerLatestLine(contentId) });

  useEffect(() => {
    const unlisten = listen(events.hookerLine, () => {
      void queryClient.invalidateQueries({ queryKey: ['overlay-line'] });
    });
    const mined = listen<{ text?: string }>(events.mined, (event) => {
      if (event.payload.text) setFeedback(t('saved'));
    });
    return () => { void unlisten.then((fn) => fn()); void mined.then((fn) => fn()); };
  }, [queryClient, t]);

  const session = tracker?.vn;
  const anime = tracker?.anime;
  const title = session?.media_title ?? anime?.media?.title ?? anime?.parsed.title ?? t('idle');
  const time = session?.elapsed_secs ?? anime?.elapsed_secs ?? 0;

  async function mine() {
    try {
      await ipc.mineLatestLine();
      setFeedback(t('saved'));
    } catch (error) {
      setFeedback(miningError(error));
    }
  }

  return (
    <div className="h-screen overflow-hidden bg-base-100 text-base-content p-3 select-none">
      <header className="flex items-center justify-between gap-2 cursor-move" data-tauri-drag-region onMouseDown={() => void getCurrentWindow().startDragging()}>
        <div className="min-w-0">
          <p className="text-[10px] uppercase tracking-wide text-base-content/50">{t('title')}</p>
          <h1 className="truncate text-sm font-semibold">{title}</h1>
        </div>
        <button type="button" className="btn btn-ghost btn-xs btn-square cursor-default" aria-label={t('hide')} onMouseDown={(event) => event.stopPropagation()} onClick={() => void updateSettings({ overlay_enabled: false })}>
          <X className="w-4 h-4" />
        </button>
        <button type="button" className="btn btn-ghost btn-xs btn-square cursor-default" aria-label={t(compact ? 'expand' : 'compact')} onMouseDown={(event) => event.stopPropagation()} onClick={() => { const next = !compact; setCompact(next); void getCurrentWindow().setSize(new LogicalSize(350, next ? 112 : 220)); }}>
          {compact ? <Maximize2 className="w-4 h-4" /> : <Minimize2 className="w-4 h-4" />}
        </button>
      </header>
      {!compact ? <>
      <div className="mt-2 flex items-center gap-3 text-xs text-base-content/70">
        <span className="font-mono tabular-nums">{formatClock(time)}</span>
        {session && session.media_type !== 'game' ? <span>{t('characters', { count: session.session_chars })}</span> : null}
        {session ? <span className={session.counting ? 'text-success' : 'text-warning'}>{session.counting ? t('counting') : t('paused')}</span> : null}
      </div>
      <p lang="ja" className="mt-2 h-9 overflow-hidden text-xs leading-relaxed text-base-content/80 selectable">{latest.data?.text ?? t('noLine')}</p>
      {feedback ? <p role="status" className="truncate text-xs text-info">{feedback}</p> : null}
      <div className="mt-2 flex gap-1.5">
        {session ? <button type="button" className="btn btn-sm" onClick={() => void ipc.setSessionPaused(!session.paused_by_user)} title={session.paused_by_user ? t('resume') : t('pause')}>
          {session.paused_by_user ? <Play className="w-4 h-4" /> : <Pause className="w-4 h-4" />}
        </button> : null}
        <button type="button" className="btn btn-sm" onClick={() => void mine()} title={t('mine')} aria-label={t('mine')}><BookmarkPlus className="w-4 h-4" /></button>
        <button type="button" className="btn btn-sm" onClick={() => void ipc.overlayOpenMain('/texthooker')} title={t('reader')} aria-label={t('reader')}><BookOpenText className="w-4 h-4" /></button>
        <button type="button" className="btn btn-sm" onClick={() => void ipc.overlayOpenMain('/log')} title={t('log')} aria-label={t('log')}><Plus className="w-4 h-4" /></button>
      </div>
      </> : <div className="mt-2 flex items-center justify-between"><span className="font-mono text-xs tabular-nums">{formatClock(time)}</span><button type="button" className="btn btn-xs" onClick={() => void mine()} aria-label={t('mine')}><BookmarkPlus className="h-3.5 w-3.5" /></button></div>}
    </div>
  );
}
