import { create } from 'zustand';

export type ToastKind = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

/** How long a toast stays up on its own. Errors stay until they are closed:
 *  the user may need to read or copy them. */
export const TOAST_DURATION_MS = 4000;
/** Older toasts make room: a burst of failures should not cover the window. */
export const MAX_TOASTS = 4;

interface ToastStore {
  toasts: Toast[];
  push: (kind: ToastKind, message: string) => number;
  dismiss: (id: number) => void;
}

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

function clearTimer(id: number) {
  const timer = timers.get(id);
  if (timer !== undefined) clearTimeout(timer);
  timers.delete(id);
}

/** Feedback for something the user just did. Each window has its own. */
export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],
  push: (kind, message) => {
    const id = nextId++;
    const kept = get().toasts.slice(-(MAX_TOASTS - 1));
    for (const dropped of get().toasts.slice(0, get().toasts.length - kept.length)) clearTimer(dropped.id);
    set({ toasts: [...kept, { id, kind, message }] });
    if (kind !== 'error') {
      timers.set(id, setTimeout(() => get().dismiss(id), TOAST_DURATION_MS));
    }
    return id;
  },
  dismiss: (id) => {
    clearTimer(id);
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  },
}));

/** Shorthands for code outside React (handlers, mutations). */
export const toast = {
  success: (message: string) => useToastStore.getState().push('success', message),
  error: (message: string) => useToastStore.getState().push('error', message),
  info: (message: string) => useToastStore.getState().push('info', message),
  warning: (message: string) => useToastStore.getState().push('warning', message),
};
