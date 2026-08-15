import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import {
  DEFAULT_LANGUAGE,
  SUPPORTED_LANGUAGES,
  isSupportedLanguage,
  type SupportedLanguage,
} from './languages';
import en from './locales/en';
import es from './locales/es';

const resources = { en, es };

/**
 * Everything is bundled — the whole app is a few hundred keys, so lazy chunks
 * would buy nothing and would risk a flash of untranslated UI on a cold start.
 */
export function initI18n(language: SupportedLanguage) {
  void i18n.use(initReactI18next).init({
    lng: language,
    fallbackLng: DEFAULT_LANGUAGE,
    supportedLngs: SUPPORTED_LANGUAGES,
    defaultNS: 'common',
    ns: Object.keys(resources[DEFAULT_LANGUAGE]),
    resources,
    returnEmptyString: false,
    interpolation: {
      // React already escapes interpolated values
      escapeValue: false,
    },
    react: { useSuspense: false },
  });

  i18n.on('languageChanged', (lng) => {
    document.documentElement.lang = lng;
  });

  return i18n;
}

export async function setAppLanguage(language: unknown) {
  if (!isSupportedLanguage(language) || i18n.language === language) return;
  await i18n.changeLanguage(language);
}

export default i18n;
