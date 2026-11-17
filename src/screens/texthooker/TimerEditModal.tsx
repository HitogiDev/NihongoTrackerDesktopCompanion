import { memo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Clock, X } from 'lucide-react';
import { Field } from '../../components/ui/Field';
import type { SessionClockSource } from './useManualTimer';
import { useModalPresence } from './useModalPresence';

/** Set the session clock by hand. */
export const TimerEditModal = memo(function TimerEditModal({
  open,
  clock,
  onClose,
  onSave,
}: {
  open: boolean;
  clock: SessionClockSource;
  onClose: () => void;
  /** Stores `total` seconds; throws if it could not. */
  onSave: (total: number) => Promise<void>;
}) {
  const { mounted, shown } = useModalPresence(open);
  return (
    <dialog
      className={`modal modal-bottom sm:modal-middle ${shown ? 'modal-open' : ''}`}
    >
      {mounted ? <TimerEditContent clock={clock} onClose={onClose} onSave={onSave} /> : null}
      <form method="dialog" className="modal-backdrop">
        <button onClick={onClose}>close</button>
      </form>
    </dialog>
  );
});

function TimerEditContent({
  clock,
  onClose,
  onSave,
}: {
  clock: SessionClockSource;
  onClose: () => void;
  onSave: (total: number) => Promise<void>;
}) {
  const { t } = useTranslation(['texthooker', 'common']);
  // Taken when the modal opens; the clock keeps running behind it.
  const [initial] = useState(() => clock.current());
  const [timerEditHours, setTimerEditHours] = useState(Math.floor(initial / 3600));
  const [timerEditMinutes, setTimerEditMinutes] = useState(Math.floor((initial % 3600) / 60));
  const [timerEditSeconds, setTimerEditSeconds] = useState(initial % 60);
  const [savingTimer, setSavingTimer] = useState(false);
  const [timerError, setTimerError] = useState(false);

  async function handleSaveTimerEdit() {
    const total = timerEditHours * 3600 + timerEditMinutes * 60 + timerEditSeconds;
    setSavingTimer(true); setTimerError(false);
    try {
      await onSave(total);
    } catch { setTimerError(true); }
    finally { setSavingTimer(false); }
  }

  return (
    <div className="modal-box max-w-sm">
      <div className="flex justify-between items-center mb-4">
        <h3 className="font-bold text-lg flex items-center gap-2">
          <Clock className="w-5 h-5 text-primary" />
          {t('timer.title')}
        </h3>
        <button
          onClick={onClose}
          className="btn btn-ghost btn-sm btn-circle"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      <div className="flex items-center justify-center gap-2 mb-6">
        <Field label={t('timer.hours')}>
          <input
            type="number"
            min="0"
            max="99"
            value={timerEditHours}
            onChange={(e) => setTimerEditHours(Math.max(0, parseInt(e.target.value) || 0))}
            className="input w-20 text-center font-mono text-lg"
          />
        </Field>
        <span className="text-2xl font-bold mt-6">:</span>
        <Field label={t('timer.minutes')}>
          <input
            type="number"
            min="0"
            max="59"
            value={timerEditMinutes}
            onChange={(e) =>
              setTimerEditMinutes(Math.min(59, Math.max(0, parseInt(e.target.value) || 0)))
            }
            className="input w-20 text-center font-mono text-lg"
          />
        </Field>
        <span className="text-2xl font-bold mt-6">:</span>
        <Field label={t('timer.seconds')}>
          <input
            type="number"
            min="0"
            max="59"
            value={timerEditSeconds}
            onChange={(e) =>
              setTimerEditSeconds(Math.min(59, Math.max(0, parseInt(e.target.value) || 0)))
            }
            className="input w-20 text-center font-mono text-lg"
          />
        </Field>
      </div>

      <div className="modal-action">
        <button onClick={onClose} className="btn btn-ghost">
          {t('common:cancel')}
        </button>
        <button disabled={savingTimer} onClick={() => void handleSaveTimerEdit()} className="btn btn-primary">
          {t('common:save')}
        </button>
      </div>
      {timerError && <p role="alert" className="mt-3 text-sm text-error">{t('timer.saveFailed')}</p>}
    </div>
  );
}

/** Confirm putting the session clock back to zero. */
export const TimerResetModal = memo(function TimerResetModal({
  open,
  onClose,
  onReset,
}: {
  open: boolean;
  onClose: () => void;
  /** Resets the clock; throws if it could not. */
  onReset: () => Promise<void>;
}) {
  const { mounted, shown } = useModalPresence(open);
  return (
    <dialog
      className={`modal modal-bottom sm:modal-middle ${
        shown ? 'modal-open' : ''
      }`}
    >
      {mounted ? <TimerResetContent onClose={onClose} onReset={onReset} /> : null}
      <form method="dialog" className="modal-backdrop">
        <button onClick={onClose}>close</button>
      </form>
    </dialog>
  );
});

function TimerResetContent({ onClose, onReset }: { onClose: () => void; onReset: () => Promise<void> }) {
  const { t } = useTranslation(['texthooker', 'common']);
  const [savingTimer, setSavingTimer] = useState(false);
  const [timerError, setTimerError] = useState(false);

  async function handleResetTimer() {
    setSavingTimer(true); setTimerError(false);
    try {
      await onReset();
    } catch { setTimerError(true); }
    finally { setSavingTimer(false); }
  }

  return (
    <div className="modal-box max-w-sm">
      <h3 className="font-bold text-lg mb-2">{t('timer.resetConfirm')}</h3>
      <p className="text-base-content/70 mb-5">
        {t('timer.resetBody')}
      </p>
      <div className="modal-action">
        <button
          type="button"
          className="btn btn-ghost"
          onClick={onClose}
        >
          {t('common:cancel')}
        </button>
        <button type="button" disabled={savingTimer} className="btn btn-error" onClick={() => void handleResetTimer()}>
          {t('timer.resetAction')}
        </button>
      </div>
      {timerError && <p role="alert" className="mt-3 text-sm text-error">{t('timer.saveFailed')}</p>}
    </div>
  );
}
