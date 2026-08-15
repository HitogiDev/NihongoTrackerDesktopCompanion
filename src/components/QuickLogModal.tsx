import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Flame, Search, TrendingUp, X } from 'lucide-react';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { Modal } from './ui/Modal';
import { MediaSearchModal } from './MediaSearchModal';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { ipc } from '../lib/ipc';
import type { LogPayload, MediaHit } from '../lib/types';

const TYPES = [
  'anime',
  'manga',
  'reading',
  'vn',
  'video',
  'movie',
  'tv show',
  'audio',
  'game',
  'book',
  'other',
] as const;

type Amount = 'episodes' | 'time' | 'chars' | 'pages';

/**
 * What each field means and which ones a type accepts, mirroring
 * `Backend/src/models/log.model.ts`. The API answers validation errors with a
 * bare `{ message: "Validation Error" }`, so the check has to happen here.
 *
 * `all` must be filled in; `any` needs at least one of them.
 */
const RULES: Record<string, { fields: Amount[]; all?: Amount[]; any?: Amount[] }> = {
  anime: { fields: ['episodes', 'time'], all: ['episodes'] },
  manga: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  reading: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  book: { fields: ['pages', 'chars', 'time'], any: ['pages', 'chars', 'time'] },
  vn: { fields: ['chars', 'time'], any: ['chars', 'time'] },
  game: { fields: ['chars', 'time'], any: ['chars', 'time'] },
  video: { fields: ['time'], all: ['time'] },
  movie: { fields: ['time'], all: ['time'] },
  audio: { fields: ['time'], all: ['time'] },
  other: { fields: ['time'], all: ['time'] },
  'tv show': { fields: ['episodes', 'time'] },
};

/** Types `/api/media/search` knows about; the rest fall back to anime. */
const SEARCHABLE = ['anime', 'vn', 'manga', 'reading', 'game', 'movie', 'tv show', 'book'];

interface Draft {
  type: string;
  description: string;
  mediaId: string;
  image: string | null;
  episodes: string;
  time: string;
  chars: string;
  pages: string;
}

const EMPTY: Draft = {
  type: 'anime',
  description: '',
  mediaId: '',
  image: null,
  episodes: '',
  time: '',
  chars: '',
  pages: '',
};

function amount(value: string): number | undefined {
  const parsed = Number(value);
  return value.trim() && Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
}

function toPayload(draft: Draft): LogPayload {
  const payload: LogPayload = {
    type: draft.type,
    // Always required, even though the schema calls it optional.
    description: draft.description.trim(),
    date: new Date().toISOString(),
  };
  if (draft.mediaId.trim()) payload.mediaId = draft.mediaId.trim();
  const rule = RULES[draft.type] ?? { fields: [] };
  for (const field of rule.fields) {
    const value = amount(draft[field]);
    if (value !== undefined) payload[field] = value;
  }
  return payload;
}

/** `null` when the draft is ready to send, otherwise the key of what's missing. */
function missing(draft: Draft): string | null {
  if (!draft.description.trim()) return 'description';
  const rule = RULES[draft.type] ?? { fields: [] };
  for (const field of rule.all ?? []) {
    if (amount(draft[field]) === undefined) return field;
  }
  if (rule.any && !rule.any.some((field) => amount(draft[field]) !== undefined)) {
    return 'any';
  }
  return null;
}

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
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    if (open) {
      setDraft(EMPTY);
      setPicking(false);
    }
  }, [open]);

  const problem = missing(draft);
  const payload = useMemo(() => toPayload(draft), [draft]);
  const type = getMediaTypeClasses(draft.type);

  // XP belongs to the server (the calculateXp middleware overwrites anything a
  // client sends), so it is asked for rather than guessed. Offline: no badge.
  const { data: preview } = useQuery({
    queryKey: ['preview-xp', 'quick', JSON.stringify(payload)],
    queryFn: () => ipc.previewXp(payload),
    enabled: open && problem === null,
    retry: false,
    staleTime: 60_000,
  });

  const save = useMutation({
    mutationFn: () => ipc.createLog(payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['recent-logs'] });
      void queryClient.invalidateQueries({ queryKey: ['streak'] });
      onClose();
    },
  });

  const rule = RULES[draft.type] ?? { fields: [] };
  const set = (patch: Partial<Draft>) => setDraft((current) => ({ ...current, ...patch }));

  return (
    <>
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
              disabled={problem !== null}
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

          <Field label={t('type')}>
            {(id) => (
              <select
                id={id}
                className="select w-full"
                value={draft.type}
                onChange={(e) => set({ type: e.target.value })}
              >
                {TYPES.map((value) => (
                  <option key={value} value={value}>
                    {t(`common:logType.${value}`)}
                  </option>
                ))}
              </select>
            )}
          </Field>

          <Field label={t('description')} hint={t('descriptionHint')}>
            {(id) => (
              <div className="flex gap-2">
                <input
                  id={id}
                  className="input flex-1"
                  value={draft.description}
                  onChange={(e) => set({ description: e.target.value })}
                  placeholder={t('descriptionPlaceholder')}
                />
                <Button appearance="outline" onClick={() => setPicking(true)}>
                  <Search className="w-4 h-4" />
                </Button>
              </div>
            )}
          </Field>

          {draft.mediaId ? (
            <div className="flex items-center gap-2 px-3 py-2 rounded-field bg-base-200/60">
              {draft.image ? (
                <img src={draft.image} alt="" className="w-8 h-11 object-cover rounded" />
              ) : null}
              <span className="min-w-0 flex-1">
                <span className={`block text-xs font-medium ${type.color}`}>{t('linked')}</span>
                <span className="block text-xs font-mono truncate text-base-content/60">
                  {draft.mediaId}
                </span>
              </span>
              <Button
                size="xs"
                appearance="ghost"
                aria-label={t('unlink')}
                onClick={() => set({ mediaId: '', image: null })}
              >
                <X className="w-3 h-3" />
              </Button>
            </div>
          ) : null}

          <div className="grid grid-cols-2 gap-3">
            {rule.fields.map((field) => (
              <Field key={field} label={t(`amount.${field}`)}>
                {(id) => (
                  <input
                    id={id}
                    type="number"
                    min={0}
                    className="input w-full"
                    value={draft[field]}
                    onChange={(e) => set({ [field]: e.target.value } as Partial<Draft>)}
                  />
                )}
              </Field>
            ))}
          </div>

          <div className="flex items-center gap-2 min-h-6">
            {problem ? (
              <span className="text-xs text-warning">
                {problem === 'any'
                  ? t('requiredAny', {
                      fields: (rule.any ?? []).map((f) => t(`amount.${f}`)).join(', '),
                    })
                  : t('required', { field: t(`amount.${problem}`, t('description')) })}
              </span>
            ) : typeof preview?.xp === 'number' ? (
              <span className={`badge badge-outline badge-sm gap-1 ${type.color}`}>
                <TrendingUp className="w-3 h-3" />
                {t('estimatedXp', { xp: preview.xp })}
              </span>
            ) : null}
          </div>

          {save.error ? (
            <p className="text-sm text-error">{String(save.error)}</p>
          ) : null}
        </div>
      </Modal>

      <MediaSearchModal
        open={picking}
        onClose={() => setPicking(false)}
        mediaType={SEARCHABLE.includes(draft.type) ? draft.type : 'anime'}
        initialQuery={draft.description}
        onPick={(hit: MediaHit, pickedType: string) => {
          set({
            type: pickedType,
            mediaId: hit.contentId,
            image: hit.contentImage ?? hit.coverImage,
            description:
              hit.title.contentTitleRomaji || hit.title.contentTitleNative || draft.description,
          });
          setPicking(false);
        }}
      />
    </>
  );
}
