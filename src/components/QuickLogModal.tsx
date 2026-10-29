import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Flame } from 'lucide-react';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import { LogForm } from './LogForm';
import { ipc } from '../lib/ipc';
import { EMPTY_DRAFT, missing, toPayload, type LogDraft } from '../lib/logRules';

export function QuickLogModal({
  open,
  onClose,
  reason = 'manual',
}: {
  open: boolean;
  onClose: () => void;
  reason?: 'manual' | 'streak';
}) {
  const { t } = useTranslation(['log', 'common']);
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<LogDraft>(EMPTY_DRAFT);

  useEffect(() => {
    if (open) setDraft(EMPTY_DRAFT);
  }, [open]);

  const save = useMutation({
    mutationFn: () => ipc.createLog(toPayload(draft)),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['recent-logs'] });
      void queryClient.invalidateQueries({ queryKey: ['streak'] });
      onClose();
    },
  });

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('title')}
      actions={
        <>
          <Button appearance="ghost" onClick={onClose}>
            {t('common:cancel')}
          </Button>
          <Button
            variant="primary"
            disabled={missing(draft) !== null}
            loading={save.isPending}
            onClick={() => save.mutate()}
          >
            {t('submit')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {reason === 'streak' ? (
          <div className="alert alert-warning py-2">
            <Flame className="w-4 h-4" />
            <span className="text-sm">{t('streakReason')}</span>
          </div>
        ) : null}

        <LogForm draft={draft} onChange={setDraft} active={open} xpKey="quick" />

        {save.error ? <p className="text-sm text-error">{String(save.error)}</p> : null}
      </div>
    </Modal>
  );
}
