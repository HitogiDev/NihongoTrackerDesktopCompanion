import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { create } from 'zustand';
import i18n from '../../../i18n';
import { events, ipc } from '../../../lib/ipc';
import type { ExtStatus } from '../../../lib/types';

export const EXT_STATUS_QUERY = ['manga', 'ext', 'status'] as const;
export const EXT_STORE_QUERY = ['manga', 'ext', 'store'] as const;
export const EXT_SOURCES_QUERY = ['manga', 'ext', 'sources'] as const;

/** The extensions server's status, kept fresh by `manga://ext-status`. */
export function useExtStatus() {
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: EXT_STATUS_QUERY, queryFn: ipc.mangaExtStatus, staleTime: 30_000 });
  useEffect(() => {
    const off = listen<ExtStatus>(events.mangaExtStatus, (event) => {
      const before = queryClient.getQueryData<ExtStatus>(EXT_STATUS_QUERY);
      queryClient.setQueryData(EXT_STATUS_QUERY, event.payload);
      // Ready again (another mode, a restart): what it lists may differ.
      if (event.payload.phase === 'ready' && before?.phase !== 'ready') {
        void queryClient.invalidateQueries({ queryKey: ['manga', 'ext'], predicate: (q) => q.queryKey[2] !== 'status' });
      }
    });
    return () => void off.then((fn) => fn());
  }, [queryClient]);
  return query;
}

export function isInstalling(status: ExtStatus | undefined): boolean {
  const phase = status?.install?.phase;
  return phase === 'downloading' || phase === 'verifying' || phase === 'unpacking';
}

/** The server can be asked now, or will start when asked. */
export function canConnect(status: ExtStatus | undefined): boolean {
  if (!status) return false;
  return status.mode === 'managed' ? status.installed && !isInstalling(status) : status.custom_configured;
}

/** The install dialog, opened from wherever extensions are first used. */
export const useExtInstallStore = create<{ open: boolean; show: () => void; close: () => void }>((set) => ({
  open: false,
  show: () => set({ open: true }),
  close: () => set({ open: false }),
}));

const KNOWN = new Set([
  'offline', 'unauthorized', 'not-installed', 'not-configured', 'start-failed', 'server', 'bad-reply', 'not-found',
  'adult-hidden', 'disk', 'unsupported', 'busy', 'other-server', 'checksum', 'cancelled', 'invalid-url',
]);

/** A Rust error code from the extensions commands, in words. */
export function extErrorText(error: unknown): string {
  const code = String(error);
  return KNOWN.has(code) ? i18n.t(`manga:ext.errors.${code}`) : i18n.t('manga:ext.errors.server');
}
