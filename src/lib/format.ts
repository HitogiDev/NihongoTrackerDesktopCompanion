import i18n from '../i18n';

export function numberWithCommas(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '0';
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** `01:23:45` — used for the live session timer. */
export function formatClock(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const seconds = s % 60;
  return [hours, minutes, seconds].map((n) => n.toString().padStart(2, '0')).join(':');
}

/** `2 h 14 min` — used for summaries. */
export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  if (hours > 0) return i18n.t('common:duration.hoursMinutes', { hours, minutes });
  if (minutes > 0) return i18n.t('common:duration.minutes', { minutes });
  return i18n.t('common:duration.seconds', { seconds: s });
}

export function formatMinutes(minutes: number): string {
  return formatDuration(minutes * 60);
}

/** Characters per hour, the metric the texthooker shows. */
export function readingSpeed(chars: number, seconds: number): number {
  if (seconds <= 0) return 0;
  return Math.round((chars * 3600) / seconds);
}

export function relativeDate(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const diff = Date.now() - then;
  const minutes = Math.round(diff / 60000);
  if (minutes < 1) return i18n.t('common:relative.now');
  if (minutes < 60) return i18n.t('common:relative.minutes', { count: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 24) return i18n.t('common:relative.hours', { count: hours });
  const days = Math.round(hours / 24);
  if (days < 30) return i18n.t('common:relative.days', { count: days });
  return new Date(iso).toLocaleDateString(i18n.language);
}

/** A type the app has no label for (a new one on the server) shows as sent,
 *  rather than as the i18n key path. */
export function logTypeLabel(type?: string | null): string {
  const key = type || 'other';
  return i18n.exists(`common:logType.${key}`) ? i18n.t(`common:logType.${key}`) : key;
}
