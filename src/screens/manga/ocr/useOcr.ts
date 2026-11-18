import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { create } from 'zustand';
import i18n from '../../../i18n';
import { events, ipc } from '../../../lib/ipc';
import type { OcrDownload, OcrStatus, OcrVolumeRun } from '../../../lib/types';
import { toast } from '../../../store/useToastStore';

export const OCR_QUERY = ['manga', 'ocr'] as const;

/** The owocr error last toasted in this window: several screens listen at once. */
let toastedError: string | null = null;

/**
 * owocr failing (not running, paused, plain text) stops the queue with the
 * error in the status: said once, wherever the user is reading.
 */
function toastOwocrError(status: OcrStatus) {
  const error = status.error;
  if (!error || !error.startsWith('owocr-')) {
    toastedError = null;
    return;
  }
  if (error === toastedError) return;
  toastedError = error;
  toast.error(ocrErrorText(error));
}

/**
 * The OCR's status, kept fresh by `manga://ocr-status` (engine and volume
 * runs) and `manga://ocr-progress` (the built-in OCR's download).
 */
export function useOcrStatus() {
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: OCR_QUERY, queryFn: ipc.mangaOcrStatus, staleTime: 30_000 });
  useEffect(() => {
    const offStatus = listen<OcrStatus>(events.mangaOcrStatus, (event) => {
      queryClient.setQueryData(OCR_QUERY, event.payload);
      toastOwocrError(event.payload);
    });
    const offProgress = listen<OcrDownload>(events.mangaOcrProgress, (event) => {
      queryClient.setQueryData<OcrStatus>(OCR_QUERY, (current) => (current ? { ...current, download: event.payload } : current));
    });
    return () => {
      void offStatus.then((fn) => fn());
      void offProgress.then((fn) => fn());
    };
  }, [queryClient]);
  return query;
}

/** A download is running now. */
export function isDownloading(status: OcrStatus | undefined): boolean {
  const phase = status?.download?.phase;
  return phase === 'downloading' || phase === 'verifying';
}

/** The run of a volume being processed, if any. */
export function runOf(status: OcrStatus | undefined, volumeId: string): OcrVolumeRun | undefined {
  return status?.runs.find((run) => run.volume_id === volumeId);
}

interface InstallStore {
  open: boolean;
  /** Once installed, what to do (process the volume that asked). */
  then: (() => void) | null;
  show: (then?: () => void) => void;
  close: () => void;
}

/** The consent dialog: any screen can ask for it before the OCR is used. */
export const useOcrInstallStore = create<InstallStore>((set) => ({
  open: false,
  then: null,
  show: (then) => set({ open: true, then: then ?? null }),
  close: () => set({ open: false, then: null }),
}));

/** An OCR error code as a sentence. */
export function ocrErrorText(error: unknown): string {
  const code = String(error);
  return i18n.exists(`manga:ocr.errors.${code}`) ? i18n.t(`manga:ocr.errors.${code}`) : i18n.t('manga:ocr.errors.error', { message: code });
}

/**
 * "Process volume": queues its pages without text, or asks to install the
 * built-in OCR first (`ready` is false only for it).
 */
export async function processVolume(volumeId: string, ready: boolean) {
  if (!ready) {
    useOcrInstallStore.getState().show(() => void processVolume(volumeId, true));
    return;
  }
  try {
    const queued = await ipc.mangaOcrProcessVolume(volumeId);
    if (queued === 0) toast.info(i18n.t('manga:ocr.toast.nothingToRead'));
  } catch (error) {
    toast.error(ocrErrorText(error));
  }
}
