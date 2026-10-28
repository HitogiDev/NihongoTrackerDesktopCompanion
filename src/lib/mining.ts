import i18n from '../i18n';

export function miningError(error: unknown): string {
  const message = String(error);
  return message.startsWith('errors.') && i18n.exists(`mining:${message}`)
    ? i18n.t(`mining:${message}`)
    : message;
}
