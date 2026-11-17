import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_TOASTS, TOAST_DURATION_MS, toast, useToastStore } from './useToastStore';

describe('useToastStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useToastStore.setState({ toasts: [] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('closes a success toast on its own', () => {
    toast.success('Saved');
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1);
    expect(useToastStore.getState().toasts).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it('keeps an error until it is dismissed', () => {
    const id = toast.error('Failed');
    vi.advanceTimersByTime(TOAST_DURATION_MS * 10);
    expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual(['Failed']);
    useToastStore.getState().dismiss(id);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it('drops the oldest toasts past the limit', () => {
    for (let i = 0; i < MAX_TOASTS + 2; i += 1) toast.error(`e${i}`);
    const messages = useToastStore.getState().toasts.map((t) => t.message);
    expect(messages).toHaveLength(MAX_TOASTS);
    expect(messages[messages.length - 1]).toBe(`e${MAX_TOASTS + 1}`);
    expect(messages[0]).toBe('e2');
  });

  it('dismissing early does not let the timer remove a later toast', () => {
    const first = toast.info('a');
    useToastStore.getState().dismiss(first);
    toast.info('b');
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1);
    expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual(['b']);
  });
});
