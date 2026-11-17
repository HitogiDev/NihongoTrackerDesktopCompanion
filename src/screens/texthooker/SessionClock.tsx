import { formatClock, numberWithCommas, readingSpeed } from '../../lib/format';
import { useSessionSeconds, type SessionClockSource } from './useManualTimer';

/** The session time as text. It follows the clock on its own, so the second
 *  that passes re-renders this text and nothing around it. */
export function SessionClock({ clock }: { clock: SessionClockSource }) {
  const seconds = useSessionSeconds(clock);
  return <>{formatClock(seconds)}</>;
}

/** Reading speed for `chars` over the session time, followed by `suffix`. */
export function SessionSpeed({
  clock,
  chars,
  suffix,
}: {
  clock: SessionClockSource;
  chars: number;
  suffix?: string;
}) {
  const seconds = useSessionSeconds(clock);
  return <>{numberWithCommas(readingSpeed(chars, seconds))}{suffix}</>;
}
