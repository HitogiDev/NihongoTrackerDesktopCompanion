import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Search } from 'lucide-react';
import { Modal } from './ui/Modal';
import { Spinner } from './ui/Spinner';
import { ipc } from '../lib/ipc';
import type { MediaHit } from '../lib/types';

const TYPES = [
  { value: 'anime', label: 'Anime' },
  { value: 'vn', label: 'Novela visual' },
  { value: 'manga', label: 'Manga' },
  { value: 'reading', label: 'Novela ligera' },
  { value: 'game', label: 'Juego' },
  { value: 'movie', label: 'Película' },
  { value: 'tv show', label: 'Serie' },
  { value: 'book', label: 'Libro' },
];

export function MediaSearchModal({
  open,
  onClose,
  mediaType = 'vn',
  initialQuery = '',
  allowTypeChange = true,
  onPick,
}: {
  open: boolean;
  onClose: () => void;
  mediaType?: string;
  initialQuery?: string;
  allowTypeChange?: boolean;
  onPick: (hit: MediaHit, type: string) => void | Promise<void>;
}) {
  const [type, setType] = useState(mediaType);
  const [query, setQuery] = useState(initialQuery);
  const [debounced, setDebounced] = useState(initialQuery);

  useEffect(() => {
    if (open) {
      setQuery(initialQuery);
      setDebounced(initialQuery);
      setType(mediaType);
    }
  }, [open, initialQuery, mediaType]);

  useEffect(() => {
    const id = setTimeout(() => setDebounced(query), 350);
    return () => clearTimeout(id);
  }, [query]);

  const { data, isFetching, error } = useQuery({
    queryKey: ['media-search', type, debounced],
    queryFn: () => ipc.searchMedia(debounced, type),
    enabled: open && debounced.trim().length >= 2,
  });

  return (
    <Modal open={open} onClose={onClose} title="Buscar media" size="lg">
      <div className="space-y-3">
        <div className="flex gap-2">
          <label className="input input-sm flex-1 flex items-center gap-2">
            <Search className="w-4 h-4 opacity-60" />
            <input
              className="grow"
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Título en romaji, inglés o japonés…"
            />
            {isFetching ? <Spinner size="xs" /> : null}
          </label>
          {allowTypeChange ? (
            <select
              className="select select-sm w-40"
              value={type}
              onChange={(e) => setType(e.target.value)}
            >
              {TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          ) : null}
        </div>

        {error ? (
          <p className="text-sm text-error">{String(error)}</p>
        ) : null}

        <div className="max-h-96 overflow-y-auto -mx-2 px-2">
          {(data ?? []).length === 0 && debounced.trim().length >= 2 && !isFetching ? (
            <p className="text-sm text-base-content/60 py-6 text-center">Sin resultados.</p>
          ) : null}
          <ul className="space-y-1">
            {(data ?? []).map((hit) => (
              <li key={`${hit.contentId}-${hit.type}`}>
                <button
                  type="button"
                  className="flex w-full items-center gap-3 rounded-field px-3 py-2 text-left transition-colors hover:bg-base-200 cursor-pointer"
                  onClick={() => void onPick(hit, type)}
                >
                  {hit.contentImage || hit.coverImage ? (
                    <img
                      src={hit.contentImage ?? hit.coverImage ?? ''}
                      alt=""
                      className="w-10 h-14 object-cover rounded shrink-0"
                    />
                  ) : (
                    <div className="w-10 h-14 rounded bg-base-300 shrink-0" />
                  )}
                  <span className="min-w-0 flex-1">
                    <span className="block font-medium truncate">
                      {hit.title.contentTitleRomaji || hit.title.contentTitleNative}
                    </span>
                    <span className="block text-xs text-base-content/60 truncate">
                      {hit.title.contentTitleNative}
                    </span>
                    <span className="block text-[11px] text-base-content/45 font-mono">
                      {hit.contentId}
                    </span>
                  </span>
                  {hit.isAdult ? <span className="badge badge-xs badge-error">18+</span> : null}
                </button>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </Modal>
  );
}
