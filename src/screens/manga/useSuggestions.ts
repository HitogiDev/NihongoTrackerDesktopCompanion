import { useEffect, useMemo } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import i18n from '../../i18n';
import { events, ipc } from '../../lib/ipc';
import type { MangaLinkSuggestion } from '../../lib/types';
import { toast } from '../../store/useToastStore';

export const SUGGESTIONS_KEY = ['manga', 'suggestions'] as const;

/**
 * The link suggestions Rust found in the background (`manga::suggest`), by
 * series id. Kept fresh by `manga://suggestions`; a library change (a link
 * made) refreshes them too, since they share the `manga` key.
 */
export function useSuggestions() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const off = listen(events.mangaSuggestions, () => {
      void queryClient.invalidateQueries({ queryKey: SUGGESTIONS_KEY });
    });
    return () => void off.then((fn) => fn());
  }, [queryClient]);
  const query = useQuery({ queryKey: SUGGESTIONS_KEY, queryFn: ipc.mangaLinkSuggestions });
  const list = query.data;
  const byId = useMemo(() => new Map((list ?? []).map((item) => [item.series_id, item])), [list]);
  return { list: list ?? [], byId };
}

/** Links each series to the media its suggestion shows. */
export async function confirmSuggestions(suggestions: MangaLinkSuggestion[]) {
  if (suggestions.length === 0) return;
  try {
    const linked = await ipc.mangaConfirmSuggestions(suggestions.map((item) => item.series_id));
    toast.success(i18n.t('manga:suggest.toast.linked', { count: linked }));
  } catch (error) {
    toast.error(i18n.t('manga:toast.linkFailed', { message: String(error) }));
  }
}

export async function dismissSuggestion(suggestion: MangaLinkSuggestion) {
  try {
    await ipc.mangaDismissSuggestion(suggestion.series_id);
  } catch (error) {
    toast.error(String(error));
  }
}

/** A score as people read it: "92 % match". */
export const scorePercent = (score: number) => Math.round(score * 100);
