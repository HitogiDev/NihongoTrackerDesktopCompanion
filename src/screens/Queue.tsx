import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import {
  Check,
  CircleAlert,
  Inbox,
  RefreshCw,
  Send,
  Trash2,
  TrendingUp,
  X,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Modal } from '../components/ui/Modal';
import { Field } from '../components/ui/Field';
import { Page, SectionTitle } from '../components/ui/Page';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { logTypeLabel, relativeDate } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import type { LogPayload, PendingItem } from '../lib/types';

export default function Queue() {
  const { t } = useTranslation(['queue', 'common']);
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<PendingItem | null>(null);

  const pending = useQuery({ queryKey: ['pending'], queryFn: ipc.listPending });
  const outbox = useQuery({ queryKey: ['outbox'], queryFn: ipc.listOutbox });

  useEffect(() => {
    const offPending = listen(events.pending, () => {
      void queryClient.invalidateQueries({ queryKey: ['pending'] });
    });
    const offOutbox = listen(events.outbox, () => {
      void queryClient.invalidateQueries({ queryKey: ['outbox'] });
    });
    return () => {
      void offPending.then((fn) => fn());
      void offOutbox.then((fn) => fn());
    };
  }, [queryClient]);

  const confirm = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload?: LogPayload }) =>
      ipc.confirmPending(id, payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['pending'] });
      void queryClient.invalidateQueries({ queryKey: ['outbox'] });
      setEditing(null);
    },
  });

  const discard = useMutation({
    mutationFn: (id: string) => ipc.discardPending(id),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['pending'] }),
  });

  const flush = useMutation({
    mutationFn: () => ipc.flushOutbox(),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['outbox'] }),
  });

  const items = pending.data ?? [];
  const outboxItems = (outbox.data ?? []).filter((i) => i.status !== 'sent');
  const sentItems = (outbox.data ?? []).filter((i) => i.status === 'sent').slice(0, 15);

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        <Button
          appearance="outline"
          size="sm"
          loading={flush.isPending}
          onClick={() => flush.mutate()}
        >
          <RefreshCw className="w-4 h-4" />
          {t('retryAll')}
        </Button>
      }
    >
      <SectionTitle aside={<span className="text-xs text-base-content/50">{items.length}</span>}>
        {t('awaiting')}
      </SectionTitle>

      {items.length === 0 ? (
        <div className="surface">
          <EmptyState icon={Inbox} title={t('empty')} description={t('emptyBody')} />
        </div>
      ) : (
        <ul className="space-y-3">
          {items.map((item) => (
            <PendingCard
              key={item.id}
              item={item}
              onConfirm={() => confirm.mutate({ id: item.id })}
              onEdit={() => setEditing(item)}
              onDiscard={() => discard.mutate(item.id)}
              busy={confirm.isPending || discard.isPending}
            />
          ))}
        </ul>
      )}

      {outboxItems.length > 0 ? (
        <>
          <SectionTitle>{t('outbox')}</SectionTitle>
          <ul className="space-y-2">
            {outboxItems.map((item) => (
              <li key={item.id} className="surface p-3 flex items-center gap-3">
                <CircleAlert
                  className={`w-4 h-4 shrink-0 ${
                    item.status === 'failed' ? 'text-error' : 'text-warning'
                  }`}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium truncate">
                    {(item.payload as LogPayload).description ?? item.kind}
                  </p>
                  <p className="text-xs text-base-content/60 truncate">
                    {item.status === 'failed' ? item.lastError : t('outboxQueued')} ·{' '}
                    {t('attempts', { count: item.attempts })}
                  </p>
                </div>
                <Button
                  size="xs"
                  appearance="outline"
                  onClick={() => void ipc.retryOutbox(item.id).then(() => outbox.refetch())}
                >
                  <Send className="w-3 h-3" />
                </Button>
                <Button
                  size="xs"
                  appearance="ghost"
                  onClick={() => void ipc.deleteOutbox(item.id).then(() => outbox.refetch())}
                >
                  <Trash2 className="w-3 h-3" />
                </Button>
              </li>
            ))}
          </ul>
        </>
      ) : null}

      {sentItems.length > 0 ? (
        <>
          <SectionTitle>{t('sent')}</SectionTitle>
          <ul className="space-y-1.5">
            {sentItems.map((item) => (
              <li
                key={item.id}
                className="flex items-center gap-3 px-3 py-2 rounded-field bg-base-100/60"
              >
                <Check className="w-4 h-4 text-success shrink-0" />
                <span className="text-sm truncate flex-1">
                  {(item.payload as LogPayload).description ?? item.kind}
                </span>
                <span className="text-xs text-base-content/50">
                  {relativeDate(item.createdAt)}
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}

      <EditPendingModal
        item={editing}
        onClose={() => setEditing(null)}
        onSave={(payload) => editing && confirm.mutate({ id: editing.id, payload })}
        saving={confirm.isPending}
      />
    </Page>
  );
}

function PendingCard({
  item,
  onConfirm,
  onEdit,
  onDiscard,
  busy,
}: {
  item: PendingItem;
  onConfirm: () => void;
  onEdit: () => void;
  onDiscard: () => void;
  busy: boolean;
}) {
  const { t } = useTranslation(['queue', 'common']);
  const payload = item.payload;
  const type = getMediaTypeClasses(payload.type);
  const detected = payload._detected;

  // The server owns XP (the calculateXp middleware overwrites whatever a client
  // sends), so ask it for the estimate instead of guessing. Offline: no badge.
  const { data: xp } = useQuery({
    queryKey: ['preview-xp', item.id],
    queryFn: () => {
      const { _detected, ...body } = payload;
      void _detected;
      return ipc.previewXp(body);
    },
    retry: false,
    staleTime: 5 * 60_000,
  });

  return (
    <li className={`surface overflow-hidden border ${type.borderColor}`}>
      <div className={`h-1 w-full ${type.accentColor}`} />
      <div className="p-4 flex items-start gap-3">
        {detected?.image ? (
          <img src={detected.image} alt="" className="w-12 h-16 object-cover rounded-field" />
        ) : null}
        <div className="min-w-0 flex-1">
          <div className={`badge badge-outline ${type.color} gap-1`}>
            <span className="text-xs font-medium">{logTypeLabel(payload.type)}</span>
          </div>
          <h3 className="font-bold mt-1.5 truncate">{payload.description}</h3>
          <div className="flex flex-wrap gap-1.5 mt-2">
            {payload.episodes ? (
              <span className={`badge badge-soft badge-sm ${type.bgColor} ${type.color}`}>
                {t('common:episodesShort', { count: payload.episodes })}
                {detected?.episode ? ` (#${detected.episode})` : ''}
              </span>
            ) : null}
            {payload.time ? (
              <span className="badge badge-soft badge-sm">
                {t('common:minutesShort', { count: payload.time })}
              </span>
            ) : null}
            {payload.chars ? (
              <span className="badge badge-soft badge-sm">
                {t('common:charsShort', { count: payload.chars })}
              </span>
            ) : null}
            {!payload.mediaId ? (
              <span className="badge badge-warning badge-sm">{t('unlinked')}</span>
            ) : null}
            {typeof xp?.xp === 'number' ? (
              <span className={`badge badge-outline badge-sm ${type.color} gap-1`}>
                <TrendingUp className="w-3 h-3" />
                {xp.xp} XP
              </span>
            ) : null}
          </div>
          {detected?.file ? (
            <p className="text-xs text-base-content/50 mt-2 truncate">{detected.file}</p>
          ) : null}
        </div>
        <div className="flex flex-col gap-1.5 shrink-0">
          <Button variant="primary" size="sm" disabled={busy} onClick={onConfirm}>
            <Check className="w-4 h-4" />
            {t('log')}
          </Button>
          <Button size="sm" appearance="outline" disabled={busy} onClick={onEdit}>
            {t('common:edit')}
          </Button>
          <Button size="sm" appearance="ghost" disabled={busy} onClick={onDiscard}>
            <X className="w-4 h-4" />
            {t('discard')}
          </Button>
        </div>
      </div>
    </li>
  );
}

function EditPendingModal({
  item,
  onClose,
  onSave,
  saving,
}: {
  item: PendingItem | null;
  onClose: () => void;
  onSave: (payload: LogPayload) => void;
  saving: boolean;
}) {
  const { t } = useTranslation(['queue', 'common']);
  const [draft, setDraft] = useState<LogPayload | null>(null);

  useEffect(() => {
    if (item) {
      const { _detected, ...rest } = item.payload;
      void _detected;
      setDraft(rest);
    }
  }, [item]);

  if (!item || !draft) return null;

  return (
    <Modal
      open={Boolean(item)}
      onClose={onClose}
      title={t('edit.title')}
      actions={
        <>
          <Button appearance="ghost" onClick={onClose}>
            {t('common:cancel')}
          </Button>
          <Button variant="primary" loading={saving} onClick={() => onSave(draft)}>
            {t('log')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label={t('edit.description')}>
          {(id) => (
            <input
              id={id}
              className="input w-full"
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            />
          )}
        </Field>
        <div className="grid grid-cols-3 gap-3">
          <Field label={t('edit.episodes')}>
            {(id) => (
              <input
                id={id}
                type="number"
                min={0}
                className="input w-full"
                value={draft.episodes ?? ''}
                onChange={(e) =>
                  setDraft({ ...draft, episodes: e.target.value ? Number(e.target.value) : undefined })
                }
              />
            )}
          </Field>
          <Field label={t('edit.minutes')}>
            {(id) => (
              <input
                id={id}
                type="number"
                min={0}
                className="input w-full"
                value={draft.time ?? ''}
                onChange={(e) =>
                  setDraft({ ...draft, time: e.target.value ? Number(e.target.value) : undefined })
                }
              />
            )}
          </Field>
          <Field label={t('edit.characters')}>
            {(id) => (
              <input
                id={id}
                type="number"
                min={0}
                className="input w-full"
                value={draft.chars ?? ''}
                onChange={(e) =>
                  setDraft({ ...draft, chars: e.target.value ? Number(e.target.value) : undefined })
                }
              />
            )}
          </Field>
        </div>
        <Field label={t('edit.mediaId')} hint={t('edit.mediaIdHint')}>
          {(id) => (
            <input
              id={id}
              className="input w-full font-mono text-sm"
              value={draft.mediaId ?? ''}
              onChange={(e) => setDraft({ ...draft, mediaId: e.target.value || undefined })}
            />
          )}
        </Field>
      </div>
    </Modal>
  );
}
