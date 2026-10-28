import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import {
  Check,
  CircleAlert,
  Inbox,
  Pencil,
  RefreshCw,
  Send,
  Trash2,
  TrendingUp,
  X,
} from 'lucide-react';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/Empty';
import { Modal } from '../components/ui/Modal';
import { Page, SectionTitle } from '../components/ui/Page';
import { MediaWebLink } from '../components/MediaWebLink';
import { LogForm } from '../components/LogForm';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { logTypeLabel, relativeDate } from '../lib/format';
import { events, ipc } from '../lib/ipc';
import { draftFromPayload, missing, toPayload, type LogDraft } from '../lib/logRules';
import { isEditableLog, outboxLabel, splitOutbox } from '../lib/queue';
import type { LogPayload, PendingItem } from '../lib/types';

/** What the editor is open on: a detected log to confirm, or one in the send queue. */
type Editing =
  | { kind: 'pending'; id: string; payload: LogPayload }
  | { kind: 'outbox'; id: string; payload: LogPayload; error: string | null };

export default function Queue() {
  const { t } = useTranslation(['queue', 'common']);
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<Editing | null>(null);

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

  const updateOutbox = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: LogPayload }) =>
      ipc.updateOutbox(id, payload),
    onSuccess: () => {
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
  const { waiting: outboxItems, sent: sentItems } = splitOutbox(outbox.data ?? []);

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
              onLog={() => setEditing({ kind: 'pending', id: item.id, payload: item.payload })}
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
                    <MediaWebLink type={(item.payload as LogPayload).type} mediaId={(item.payload as LogPayload).mediaId}>
                      {outboxLabel(item)}
                    </MediaWebLink>
                  </p>
                  <p className="text-xs text-base-content/60 truncate">
                    {item.status === 'failed' ? item.lastError : t('outboxQueued')} ·{' '}
                    {t('attempts', { count: item.attempts })}
                  </p>
                </div>
                {isEditableLog(item) ? (
                  <Button
                    size="xs"
                    appearance="ghost"
                    aria-label={t('edit.title')}
                    onClick={() =>
                      setEditing({
                        kind: 'outbox',
                        id: item.id,
                        payload: item.payload,
                        error: item.status === 'failed' ? item.lastError : null,
                      })
                    }
                  >
                    <Pencil className="w-3 h-3" />
                  </Button>
                ) : null}
                <Button
                  size="xs"
                  appearance="outline"
                  aria-label={t('retry')}
                  onClick={() => void ipc.retryOutbox(item.id).then(() => outbox.refetch())}
                >
                  <Send className="w-3 h-3" />
                </Button>
                <Button
                  size="xs"
                  appearance="ghost"
                  aria-label={t('common:delete')}
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
                  <MediaWebLink type={(item.payload as LogPayload).type} mediaId={(item.payload as LogPayload).mediaId}>
                    {outboxLabel(item)}
                  </MediaWebLink>
                </span>
                <span className="text-xs text-base-content/50">
                  {relativeDate(item.createdAt)}
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}

      <EditLogModal
        editing={editing}
        onClose={() => setEditing(null)}
        onSave={(payload) => {
          if (!editing) return;
          if (editing.kind === 'pending') confirm.mutate({ id: editing.id, payload });
          else updateOutbox.mutate({ id: editing.id, payload });
        }}
        saving={confirm.isPending || updateOutbox.isPending}
        saveError={editing?.kind === 'outbox' ? updateOutbox.error : confirm.error}
      />
    </Page>
  );
}

function PendingCard({
  item,
  onLog,
  onDiscard,
  busy,
}: {
  item: PendingItem;
  onLog: () => void;
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
          <h3 className="font-bold mt-1.5 truncate">
            <MediaWebLink type={payload.type} mediaId={payload.mediaId}>{payload.mediaTitle || payload.description}</MediaWebLink>
          </h3>
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
          <Button variant="primary" size="sm" disabled={busy} onClick={onLog}>
            <Check className="w-4 h-4" />
            {t('log')}
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

function EditLogModal({
  editing,
  onClose,
  onSave,
  saving,
  saveError,
}: {
  editing: Editing | null;
  onClose: () => void;
  onSave: (payload: LogPayload) => void;
  saving: boolean;
  saveError: unknown;
}) {
  const { t } = useTranslation(['queue', 'common']);
  const [draft, setDraft] = useState<LogDraft | null>(null);

  useEffect(() => {
    setDraft(editing ? draftFromPayload(editing.payload) : null);
  }, [editing]);

  if (!editing || !draft) return null;

  return (
    <Modal
      open
      onClose={onClose}
      title={t('edit.title')}
      actions={
        <>
          <Button appearance="ghost" onClick={onClose}>
            {t('common:cancel')}
          </Button>
          <Button
            variant="primary"
            loading={saving}
            disabled={missing(draft) !== null}
            onClick={() => onSave(toPayload(draft))}
          >
            {editing.kind === 'pending' ? t('log') : t('edit.saveAndSend')}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        {editing.kind === 'outbox' && editing.error ? (
          <div className="alert alert-error py-2">
            <CircleAlert className="w-4 h-4" />
            <span className="text-sm">{t('edit.rejected', { error: editing.error })}</span>
          </div>
        ) : null}
        <LogForm draft={draft} onChange={setDraft} showDate xpKey={`${editing.kind}:${editing.id}`} />
        {saveError ? <p className="text-sm text-error">{String(saveError)}</p> : null}
      </div>
    </Modal>
  );
}
