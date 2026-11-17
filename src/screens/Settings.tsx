import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Page } from '../components/ui/Page';
import { useAppStore } from '../store/useAppStore';
import { AccountSection } from './settings/AccountSection';
import { AdvancedSection } from './settings/AdvancedSection';
import { AnimeSection } from './settings/AnimeSection';
import { AnkiSection } from './settings/AnkiSection';
import { AppSection } from './settings/AppSection';
import { BackupSection } from './settings/BackupSection';
import { DictionariesSection } from './settings/DictionariesSection';
import { DiscordSection } from './settings/DiscordSection';
import { NovelsSection } from './settings/NovelsSection';
import { ReadingOverlaySection } from './settings/ReadingOverlaySection';
import { SettingsSearch } from './settings/SettingsSearch';
import { SECTION_ICONS, SECTION_IDS, isSectionId, type SectionId } from './settings/sections';
import { useSettingHighlight } from './settings/useSettingHighlight';

const SECTION_KEY = 'settings:section';

function storedSection(): SectionId {
  try {
    const value = localStorage.getItem(SECTION_KEY);
    return isSectionId(value) ? value : 'account';
  } catch {
    return 'account';
  }
}

export default function SettingsScreen() {
  const { t } = useTranslation(['settings', 'common']);
  const settings = useAppStore((s) => s.settings);
  const [section, setSection] = useState<SectionId>(storedSection);
  const tabs = useRef<Partial<Record<SectionId, HTMLButtonElement | null>>>({});
  const panel = useRef<HTMLElement>(null);
  const request = useSettingHighlight(panel);

  useEffect(() => {
    try {
      localStorage.setItem(SECTION_KEY, section);
    } catch {
      // Private storage: the window just opens on the first section next time.
    }
  }, [section]);

  if (!settings) return null;

  const select = (id: SectionId, focus = false) => {
    setSection(id);
    if (focus) tabs.current[id]?.focus();
  };

  // A vertical tab list: arrows move between sections, Home/End to the ends.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = SECTION_IDS.indexOf(section);
    const last = SECTION_IDS.length - 1;
    const next = {
      ArrowDown: index === last ? 0 : index + 1,
      ArrowRight: index === last ? 0 : index + 1,
      ArrowUp: index === 0 ? last : index - 1,
      ArrowLeft: index === 0 ? last : index - 1,
      Home: 0,
      End: last,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    select(SECTION_IDS[next], true);
  };

  return (
    <Page title={t('title')} description={t('subtitle')}>
      <div className="flex flex-col gap-5 sm:flex-row sm:items-start">
        <nav className="shrink-0 space-y-3 sm:sticky sm:top-20 sm:w-52" aria-label={t('sections.label')}>
          <SettingsSearch
            onPick={(target, label) => {
              setSection(target);
              request(label);
            }}
          />
          <div
            role="tablist"
            aria-orientation="vertical"
            aria-label={t('sections.label')}
            className="flex gap-1 overflow-x-auto pb-1 sm:flex-col sm:overflow-visible sm:pb-0"
            onKeyDown={onKeyDown}
          >
            {SECTION_IDS.map((id) => {
              const Icon = SECTION_ICONS[id];
              const active = id === section;
              return (
                <button
                  key={id}
                  ref={(element) => {
                    tabs.current[id] = element;
                  }}
                  id={`settings-tab-${id}`}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  aria-controls="settings-panel"
                  tabIndex={active ? 0 : -1}
                  onClick={() => select(id)}
                  className={active
                    ? 'flex shrink-0 items-center gap-2.5 whitespace-nowrap rounded-field bg-base-100 px-3 py-2 text-left text-sm font-medium text-base-content shadow-sm'
                    : 'flex shrink-0 items-center gap-2.5 whitespace-nowrap rounded-field px-3 py-2 text-left text-sm text-base-content/70 transition-colors hover:bg-base-200 hover:text-base-content'}
                >
                  <Icon className={active ? 'h-4 w-4 shrink-0 text-primary' : 'h-4 w-4 shrink-0'} />
                  {t(`sections.${id}`)}
                </button>
              );
            })}
          </div>
        </nav>

        <section
          ref={panel}
          id="settings-panel"
          role="tabpanel"
          aria-labelledby={`settings-tab-${section}`}
          className="min-w-0 flex-1 space-y-4"
          // One panel per section, so its scroll and focus state start fresh.
          key={section}
        >
          <h2 className="text-lg font-semibold">{t(`sections.${section}`)}</h2>
          {section === 'account' ? <AccountSection settings={settings} /> : null}
          {section === 'anime' ? <AnimeSection settings={settings} /> : null}
          {section === 'novels' ? <NovelsSection settings={settings} /> : null}
          {section === 'readingOverlay' ? <ReadingOverlaySection /> : null}
          {section === 'dictionaries' ? <DictionariesSection /> : null}
          {section === 'anki' ? <AnkiSection settings={settings} /> : null}
          {section === 'discord' ? <DiscordSection settings={settings} /> : null}
          {section === 'app' ? <AppSection settings={settings} /> : null}
          {section === 'backup' ? <BackupSection /> : null}
          {section === 'advanced' ? <AdvancedSection settings={settings} /> : null}
        </section>
      </div>
    </Page>
  );
}
