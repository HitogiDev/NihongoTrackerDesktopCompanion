import { create } from 'zustand';

interface QuickLogStore {
  open: boolean;
  /** Where the form was opened from, so it can explain itself. */
  reason: 'manual' | 'streak';
  openForm: (reason?: 'manual' | 'streak') => void;
  closeForm: () => void;
}

/**
 * The manual log form lives outside any screen: a notification click or the
 * tray can ask for it while the user is anywhere in the app.
 */
export const useQuickLogStore = create<QuickLogStore>((set) => ({
  open: false,
  reason: 'manual',
  openForm: (reason = 'manual') => set({ open: true, reason }),
  closeForm: () => set({ open: false }),
}));
