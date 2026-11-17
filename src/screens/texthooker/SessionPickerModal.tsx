import { memo, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { formatClock, numberWithCommas } from '../../lib/format';
import { ipc } from '../../lib/ipc';
import type { TextSessionSummary } from '../../lib/types';
import { LOCAL_SESSION } from './constants';
import { useModalPresence } from './useModalPresence';
import { ModalBackdrop } from '../../components/ui/Modal';

interface SessionPickerProps {
  open: boolean;
  currentId: string;
  onClose: () => void;
  onPick: (session: TextSessionSummary) => void | Promise<void>;
  onSearch: () => void;
}

/** Every session stored locally, plus the free-form one. */
export const SessionPickerModal = memo(function SessionPickerModal(props: SessionPickerProps) {
  const { open, onClose } = props;
  const { mounted, shown } = useModalPresence(open);
  return (
    <dialog className={`modal modal-bottom sm:modal-middle ${shown ? 'modal-open' : ''}`}>
      {mounted ? <SessionPickerContent {...props} /> : null}
      <ModalBackdrop onClose={onClose} />
    </dialog>
  );
});

function SessionPickerContent({ open, currentId, onClose, onPick, onSearch }: SessionPickerProps) {
  const { t } = useTranslation('texthooker');
  const { data } = useQuery({
    queryKey: ['text-sessions'],
    queryFn: ipc.listTextSessions,
    enabled: open,
  });

  const rows = useMemo(() => {
    const sessions = data ?? [];
    if (sessions.some((s) => s.contentId === LOCAL_SESSION)) return sessions;
    return [
      {
        contentId: LOCAL_SESSION,
        mediaTitle: null,
        mediaType: null,
        timerSeconds: 0,
        lines: 0,
        chars: 0,
        sessionChars: 0,
        sessionLines: 0,
      },
      ...sessions,
    ];
  }, [data]);

  return (
    <div className="modal-box max-w-2xl">
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-bold text-lg">{t('sessions.title')}</h3>
        <button onClick={onClose} className="btn btn-ghost btn-sm btn-circle">
          <X className="w-5 h-5" />
        </button>
      </div>
      <ul className="max-h-96 overflow-y-auto space-y-1">
        {rows.map((session) => (
          <li key={session.contentId}>
            <button
              type="button"
              className={`flex w-full items-center gap-3 rounded-field px-3 py-2 text-left transition-colors cursor-pointer ${
                session.contentId === currentId
                  ? 'bg-primary text-primary-content'
                  : 'hover:bg-base-200'
              }`}
              onClick={() => void onPick(session)}
            >
              <span className="min-w-0 flex-1">
                <span className="block font-medium truncate">
                  {session.contentId === LOCAL_SESSION
                    ? t('sessions.free')
                    : (session.mediaTitle ?? session.contentId)}
                </span>
                <span className="block text-xs opacity-70 font-mono">{session.contentId}</span>
              </span>
              <span className="text-xs tabular-nums opacity-80 shrink-0">
                {numberWithCommas(session.chars)} car. · {formatClock(session.timerSeconds)}
              </span>
            </button>
          </li>
        ))}
      </ul>
      <div className="modal-action">
        <button type="button" className="btn btn-primary" onClick={onSearch}>
          {t('sessions.linkAnother')}
        </button>
      </div>
    </div>
  );
}
