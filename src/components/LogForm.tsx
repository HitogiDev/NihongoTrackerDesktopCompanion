import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Search, TrendingUp, X } from 'lucide-react';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { MediaSearchModal } from './MediaSearchModal';
import { getMediaTypeClasses } from '../constants/mediaColors';
import { ipc } from '../lib/ipc';
import type { MediaHit } from '../lib/types';
import { LOG_TYPES, SEARCHABLE, missing, ruleFor, toPayload, type LogDraft } from '../lib/logRules';

/**
 * Every field of a log, shared by the quick-log modal and the queue editor so
 * both validate the same way. The parent owns the draft and the submit button;
 * `missing(draft)` says whether it may be sent.
 */
export function LogForm({
  draft,
  onChange,
  showDate = false,
  active = true,
  xpKey,
}: {
  draft: LogDraft;
  onChange: (draft: LogDraft) => void;
  showDate?: boolean;
  /** Off while the surrounding modal is closed, so no XP preview is asked for. */
  active?: boolean;
  /** Distinguishes XP previews of different logs in the query cache. */
  xpKey: string;
}) {
  const { t } = useTranslation(['log', 'common']);
  const [picking, setPicking] = useState(false);
  // A blank form is not a mistake yet: what is missing shows once the user has
  // typed something, or right away for a log that already has a title (the
  // queue editor opens one the server turned down).
  const [edited, setEdited] = useState(false);

  const problem = missing(draft);
  const rule = ruleFor(draft.type);
  const type = getMediaTypeClasses(draft.type);
  const set = (patch: Partial<LogDraft>) => {
    setEdited(true);
    onChange({ ...draft, ...patch });
  };
  const showProblem = edited || draft.mediaTitle.trim() !== '';

  // XP belongs to the server (the calculateXp middleware overwrites anything a
  // client sends), so it is asked for rather than guessed. Offline: no badge.
  const payload = toPayload(draft, new Date(0));
  const { data: preview } = useQuery({
    queryKey: ['preview-xp', xpKey, JSON.stringify(payload)],
    queryFn: () => ipc.previewXp(toPayload(draft)),
    enabled: active && problem === null,
    retry: false,
    staleTime: 60_000,
  });

  return (
    <>
      <div className="space-y-3">
        <Field label={t('type')}>
          {(id) => (
            <select
              id={id}
              className="select w-full"
              value={draft.type}
              onChange={(e) => set({ type: e.target.value })}
            >
              {LOG_TYPES.map((value) => (
                <option key={value} value={value}>
                  {t(`common:logType.${value}`)}
                </option>
              ))}
            </select>
          )}
        </Field>

        <Field label={t('mediaTitle')} hint={t('mediaTitleHint')}>
          {(id) => (
            <div className="flex gap-2">
              <input
                id={id}
                className="input flex-1"
                value={draft.mediaTitle}
                onChange={(e) => set({ mediaTitle: e.target.value })}
                placeholder={t('mediaTitlePlaceholder')}
              />
              <Button appearance="outline" aria-label={t('search')} onClick={() => setPicking(true)}>
                <Search className="w-4 h-4" />
              </Button>
            </div>
          )}
        </Field>

        <Field label={t('comment')} hint={t('commentHint')}>
          {(id) => (
            <textarea
              id={id}
              className="textarea w-full"
              rows={3}
              value={draft.comment}
              onChange={(e) => set({ comment: e.target.value })}
              placeholder={t('commentPlaceholder')}
            />
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
                  onChange={(e) => set({ [field]: e.target.value } as Partial<LogDraft>)}
                />
              )}
            </Field>
          ))}
          {showDate ? (
            <Field label={t('date')} hint={t('dateHint')}>
              {(id) => (
                <input
                  id={id}
                  type="datetime-local"
                  className="input w-full"
                  value={draft.date}
                  onChange={(e) => set({ date: e.target.value })}
                />
              )}
            </Field>
          ) : null}
        </div>

        <div className="flex items-center gap-2 min-h-6">
          {problem ? (showProblem ? (
            <span className="text-xs text-warning">
              {problem === 'any'
                ? t('requiredAny', {
                    fields: (rule.any ?? []).map((f) => t(`amount.${f}`)).join(', '),
                  })
                : t('required', {
                    field: problem === 'mediaTitle' ? t('mediaTitle') : t(`amount.${problem}`),
                  })}
            </span>
          ) : null) : typeof preview?.xp === 'number' ? (
            <span className={`badge badge-outline badge-sm gap-1 ${type.color}`}>
              <TrendingUp className="w-3 h-3" />
              {t('estimatedXp', { xp: preview.xp })}
            </span>
          ) : null}
        </div>
      </div>

      <MediaSearchModal
        open={picking}
        onClose={() => setPicking(false)}
        mediaType={SEARCHABLE.includes(draft.type) ? draft.type : 'anime'}
        initialQuery={draft.mediaTitle}
        onPick={(hit: MediaHit, pickedType: string) => {
          set({
            type: pickedType,
            mediaId: hit.contentId,
            image: hit.contentImage ?? hit.coverImage,
            mediaTitle:
              hit.title.contentTitleRomaji || hit.title.contentTitleNative || draft.mediaTitle,
          });
          setPicking(false);
        }}
      />
    </>
  );
}
