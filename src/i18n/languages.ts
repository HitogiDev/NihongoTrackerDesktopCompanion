/**
 * UI languages the companion ships translations for. Same pair the web app
 * supports (`NihongoTracker/Frontend/src/i18n/languages.ts`), so an account's
 * language means the same thing in both.
 */
export const SUPPORTED_LANGUAGES = ['en', 'es'] as const;

export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

export const DEFAULT_LANGUAGE: SupportedLanguage = 'en';

/** Each language is always labelled in its own language, never translated. */
export const LANGUAGE_LABELS: Record<SupportedLanguage, string> = {
  en: 'English',
  es: 'Español',
};

export function isSupportedLanguage(value: unknown): value is SupportedLanguage {
  return (
    typeof value === 'string' && (SUPPORTED_LANGUAGES as readonly string[]).includes(value)
  );
}

/** The OS language, when we ship it; otherwise English. */
export function detectLanguage(): SupportedLanguage {
  try {
    const browser = navigator.language?.split('-')[0];
    if (isSupportedLanguage(browser)) return browser;
  } catch {
    // navigator can be missing in exotic environments
  }
  return DEFAULT_LANGUAGE;
}
