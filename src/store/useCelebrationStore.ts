import { create } from 'zustand';
import type { LogCelebration } from '../lib/types';

interface CelebrationState {
  pending: LogCelebration | null;
  achievements: number;
  celebrate: (payload?: LogCelebration, achievements?: number) => void;
  close: () => void;
}

/**
 * Port of NihongoTracker's `store/logCelebration.ts`, including the coalescing:
 * a batch of logs must not queue up a dozen overlays, so their XP is summed and
 * the level-up bounds are widened to cover the whole batch.
 */
export const useCelebrationStore = create<CelebrationState>((set) => ({
  pending: null,
  achievements: 0,

  celebrate: (payload, achievements = 0) =>
    set((state) => {
      if (!payload) {
        return achievements > 0
          ? { achievements: state.achievements + achievements }
          : state;
      }
      if (!state.pending) {
        return { pending: payload, achievements: state.achievements + achievements };
      }

      const merged: LogCelebration = {
        ...payload,
        xpGained: state.pending.xpGained + payload.xpGained,
        levelUp:
          state.pending.levelUp || payload.levelUp
            ? {
                from:
                  state.pending.levelUp?.from ?? payload.levelUp?.from ?? payload.xp.level,
                to: payload.levelUp?.to ?? state.pending.levelUp?.to ?? payload.xp.level,
              }
            : undefined,
      };
      if (merged.levelUp && merged.levelUp.to <= merged.levelUp.from) {
        merged.levelUp = undefined;
      }
      return { pending: merged, achievements: state.achievements + achievements };
    }),

  close: () => set({ pending: null, achievements: 0 }),
}));

/** Push a celebration from anywhere. */
export function celebrateLog(payload?: LogCelebration, achievements = 0) {
  useCelebrationStore.getState().celebrate(payload, achievements);
}
