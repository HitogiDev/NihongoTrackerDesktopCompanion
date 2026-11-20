import { useEffect, useRef, useState, type FocusEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ArrowLeftRight,
  BookOpen,
  Columns2,
  Expand,
  Keyboard,
  RectangleVertical,
  Rows3,
  Settings2,
  Shrink,
  SkipBack,
  SkipForward,
  X,
  type LucideIcon,
} from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { mangaUrl } from '../../../lib/manga';
import type { MangaVolume, PageFit, ReaderBackground, ReaderLayout, ReaderPrefs, ReadingDirection, TextLayerMode } from '../../../lib/types';
import { BACKGROUND_CLASS } from './styles';

const BAR = 'pointer-events-auto absolute inset-x-0 z-30 border-base-300/60 bg-base-100/85 text-base-content shadow-sm backdrop-blur-md motion-safe:transition motion-safe:duration-200 motion-safe:ease-out';
const TOP_SHOWN = 'top-0 border-b translate-y-0 opacity-100';
const TOP_HIDDEN = 'top-0 border-b -translate-y-full opacity-0 pointer-events-none';
const BOTTOM_SHOWN = 'bottom-0 border-t translate-y-0 opacity-100';
const BOTTOM_HIDDEN = 'bottom-0 border-t translate-y-full opacity-0 pointer-events-none';

/** Focus from the keyboard (Tab) brings a hidden bar up, and keeps it up
 *  while it stays inside; a click's focus does not. */
function keyboardFocus(onFocus: (inside: boolean) => void) {
  return {
    onFocus: (event: FocusEvent<HTMLElement>) => {
      if (event.target.matches(':focus-visible')) onFocus(true);
    },
    onBlur: (event: FocusEvent<HTMLElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) onFocus(false);
    },
  };
}

function Segmented<T extends string>({ label, value, options, onChange }: {
  label: string;
  value: T;
  options: { value: T; label: string; icon?: LucideIcon }[];
  onChange: (value: T) => void;
}) {
  return (
    <div>
      <p className="mb-1.5 text-xs font-medium text-base-content/60">{label}</p>
      <div className="join w-full" role="radiogroup" aria-label={label}>
        {options.map(({ value: option, label: text, icon: Icon }) => (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={value === option}
            className={value === option ? 'btn btn-sm btn-primary join-item flex-1 gap-1.5 px-2' : 'btn btn-sm join-item flex-1 gap-1.5 px-2'}
            onClick={() => onChange(option)}
          >
            {Icon ? <Icon className="h-3.5 w-3.5" aria-hidden="true" /> : null}
            <span className="truncate">{text}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

const BACKGROUNDS: ReaderBackground[] = ['black', 'gray', 'paper'];

export function SettingsPanel({ prefs, onChange, hasText, onShortcuts }: {
  prefs: ReaderPrefs;
  onChange: (patch: Partial<ReaderPrefs>) => void;
  hasText: boolean;
  onShortcuts: () => void;
}) {
  const { t } = useTranslation('mangaReader');
  return (
    <div className="surface-raised pointer-events-auto absolute right-3 top-14 z-40 w-80 space-y-4 p-4 text-base-content" role="dialog" aria-label={t('settings.title')}>
      <Segmented<ReaderLayout>
        label={t('settings.layout')}
        value={prefs.layout}
        onChange={(layout) => onChange({ layout })}
        options={[
          { value: 'single', label: t('layout.single'), icon: RectangleVertical },
          { value: 'double', label: t('layout.double'), icon: Columns2 },
          { value: 'vertical', label: t('layout.vertical'), icon: Rows3 },
        ]}
      />
      {prefs.layout === 'double' ? (
        <label className="flex cursor-default items-center justify-between gap-3 text-sm">
          <span>
            {t('settings.coverAlone')}
            <span className="block text-xs text-base-content/55">{t('settings.coverAloneHint')}</span>
          </span>
          <input type="checkbox" className="toggle toggle-sm toggle-primary" checked={prefs.cover_alone} onChange={(event) => onChange({ cover_alone: event.target.checked })} />
        </label>
      ) : null}
      <Segmented<ReadingDirection>
        label={t('settings.direction')}
        value={prefs.direction}
        onChange={(direction) => onChange({ direction })}
        options={[
          { value: 'rtl', label: t('direction.rtl') },
          { value: 'ltr', label: t('direction.ltr') },
        ]}
      />
      <Segmented<PageFit>
        label={t('settings.fit')}
        value={prefs.fit}
        onChange={(fit) => onChange({ fit })}
        options={[
          { value: 'height', label: t('fit.height') },
          { value: 'width', label: t('fit.width') },
          { value: 'original', label: t('fit.original') },
        ]}
      />
      <div>
        <p className="mb-1.5 text-xs font-medium text-base-content/60">{t('settings.background')}</p>
        <div className="flex gap-2" role="radiogroup" aria-label={t('settings.background')}>
          {BACKGROUNDS.map((background) => (
            <button
              key={background}
              type="button"
              role="radio"
              aria-checked={prefs.background === background}
              title={t(`background.${background}`)}
              aria-label={t(`background.${background}`)}
              onClick={() => onChange({ background })}
              className={prefs.background === background
                ? `h-8 flex-1 rounded-field border border-base-300 ring-2 ring-primary ring-offset-2 ring-offset-base-100 ${BACKGROUND_CLASS[background]}`
                : `h-8 flex-1 rounded-field border border-base-300 ${BACKGROUND_CLASS[background]}`}
            />
          ))}
        </div>
      </div>
      {hasText ? (
        <Segmented<TextLayerMode>
          label={t('settings.textLayer')}
          value={prefs.text_layer}
          onChange={(text_layer) => onChange({ text_layer })}
          options={[
            { value: 'hover', label: t('text.hover') },
            { value: 'invisible', label: t('text.invisible') },
            { value: 'always', label: t('text.always') },
            { value: 'hidden', label: t('text.hidden') },
          ]}
        />
      ) : null}
      <div className="border-t border-base-300/60 pt-3">
        <button
          type="button"
          className="btn btn-ghost btn-sm -mx-2 w-[calc(100%+1rem)] justify-between px-2 font-normal"
          onClick={onShortcuts}
        >
          <span className="flex items-center gap-2">
            <Keyboard className="h-4 w-4" aria-hidden="true" />
            {t('shortcuts.title')}
          </span>
          <kbd className="kbd kbd-xs">?</kbd>
        </button>
      </div>
    </div>
  );
}

export function TopBar({ visible, title, subtitle, fullscreen, onClose, onFullscreen, settingsOpen, onSettings, onShortcuts, onHover, onFocusWithin, extra }: {
  visible: boolean;
  title: string;
  subtitle: string;
  fullscreen: boolean;
  onClose: () => void;
  onFullscreen: () => void;
  settingsOpen: boolean;
  onSettings: () => void;
  onShortcuts: () => void;
  onHover: (hovering: boolean) => void;
  /** Tab reached the bar, or left it. */
  onFocusWithin: (inside: boolean) => void;
  extra?: ReactNode;
}) {
  const { t } = useTranslation('mangaReader');
  return (
    <header className={`${BAR} ${visible ? TOP_SHOWN : TOP_HIDDEN}`} data-reader-chrome onMouseEnter={() => onHover(true)} onMouseLeave={() => onHover(false)} {...keyboardFocus(onFocusWithin)}>
      <div className="flex h-12 items-center gap-2 px-2">
        <Button appearance="ghost" size="sm" shape="square" aria-label={t('close')} title={t('close')} onClick={onClose}>
          <X className="h-4 w-4" />
        </Button>
        <div className="min-w-0 flex-1 leading-tight">
          <p className="truncate text-sm font-semibold">{title}</p>
          <p className="truncate text-xs text-base-content/60">{subtitle}</p>
        </div>
        {extra}
        <Button appearance="ghost" size="sm" shape="square" className="hidden sm:inline-flex" aria-label={t('shortcuts.title')} title={t('shortcuts.open')} onClick={onShortcuts}>
          <Keyboard className="h-4 w-4" />
        </Button>
        <Button appearance="ghost" size="sm" shape="square" aria-label={fullscreen ? t('exitFullscreen') : t('fullscreen')} title={`${fullscreen ? t('exitFullscreen') : t('fullscreen')} (F)`} onClick={onFullscreen}>
          {fullscreen ? <Shrink className="h-4 w-4" /> : <Expand className="h-4 w-4" />}
        </Button>
        <Button appearance="ghost" size="sm" shape="square" className={settingsOpen ? 'btn-active' : ''} aria-label={t('settings.title')} aria-expanded={settingsOpen} title={t('settings.title')} onClick={onSettings}>
          <Settings2 className="h-4 w-4" />
        </Button>
      </div>
    </header>
  );
}

/** A button to the volume before or after, disabled when there is none. */
function VolumeButton({ volume, label, icon: Icon, onOpen }: {
  volume: MangaVolume | null;
  label: (title: string) => string;
  icon: LucideIcon;
  onOpen: () => void;
}) {
  const { t } = useTranslation('mangaReader');
  const text = volume ? label(volume.title) : t('volumeNav.none');
  return (
    <Button appearance="ghost" size="sm" shape="square" disabled={!volume} aria-label={text} title={text} onClick={onOpen}>
      <Icon className="h-4 w-4" />
    </Button>
  );
}

export function BottomBar({ visible, volumeId, page, shown, pageLabel, pageCount, direction, previousVolume, nextVolume, onPreviousVolume, onNextVolume, onSeek, onHover, onFocusWithin }: {
  visible: boolean;
  volumeId: string;
  page: number;
  /** Every page on screen (two in a spread). */
  shown: number[];
  pageLabel: string;
  pageCount: number;
  direction: ReadingDirection;
  /** The volumes before and after this one in the series. */
  previousVolume: MangaVolume | null;
  nextVolume: MangaVolume | null;
  onPreviousVolume: () => void;
  onNextVolume: () => void;
  onSeek: (page: number) => void;
  onHover: (hovering: boolean) => void;
  onFocusWithin: (inside: boolean) => void;
}) {
  const { t } = useTranslation('mangaReader');
  const [strip, setStrip] = useState(false);
  const current = useRef<HTMLButtonElement | null>(null);
  const rtl = direction === 'rtl';
  // On the side the slider fills towards: a right-to-left book's next
  // volume is on the left, like its next page.
  const previous = (
    <VolumeButton
      volume={previousVolume}
      label={(title) => t('volumeNav.previous', { volume: title })}
      icon={rtl ? SkipForward : SkipBack}
      onOpen={onPreviousVolume}
    />
  );
  const next = (
    <VolumeButton
      volume={nextVolume}
      label={(title) => t('volumeNav.next', { volume: title })}
      icon={rtl ? SkipBack : SkipForward}
      onOpen={onNextVolume}
    />
  );

  useEffect(() => {
    if (strip) current.current?.scrollIntoView({ block: 'nearest', inline: 'center' });
  }, [strip, page]);

  // A mouse wheel only scrolls up and down: over the strip it scrolls it
  // sideways, forward in the book's direction. Native and not passive, so
  // the page under it does not get the wheel too.
  const list = useRef<HTMLOListElement | null>(null);
  useEffect(() => {
    const element = list.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      event.preventDefault();
      const unit = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? element.clientWidth : 1;
      element.scrollBy({ left: event.deltaY * unit * (rtl ? -1 : 1) });
    };
    element.addEventListener('wheel', onWheel, { passive: false });
    return () => element.removeEventListener('wheel', onWheel);
  }, [rtl]);

  return (
    <footer
      className={`${BAR} ${visible ? BOTTOM_SHOWN : BOTTOM_HIDDEN}`}
      data-reader-chrome
      onMouseEnter={() => { setStrip(true); onHover(true); }}
      onMouseLeave={() => { setStrip(false); onHover(false); }}
      {...keyboardFocus((inside) => { setStrip(inside); onFocusWithin(inside); })}
    >
      <div className={strip ? 'grid grid-rows-[1fr] motion-safe:transition-[grid-template-rows] motion-safe:duration-200' : 'grid grid-rows-[0fr] motion-safe:transition-[grid-template-rows] motion-safe:duration-200'}>
        <div className="overflow-hidden">
          <ol ref={list} className="flex gap-2 overflow-x-auto px-4 pb-1 pt-3" dir={rtl ? 'rtl' : 'ltr'} aria-label={t('pages')}>
            {strip ? Array.from({ length: pageCount }, (_, index) => (
              // Thumbnails (`manga://strip`, 240 px wide). A page of a volume
              // on a server that is not here yet comes as it is: it is never
              // downloaded only to be shrunk. The ones scrolled away skip
              // layout and paint.
              <li key={index} className="shrink-0 [contain-intrinsic-size:auto_4rem_auto_7.5rem] [content-visibility:auto]">
                <button
                  ref={index === page ? current : undefined}
                  type="button"
                  onClick={() => onSeek(index)}
                  aria-label={t('goToPage', { page: index + 1 })}
                  aria-current={shown.includes(index) ? 'page' : undefined}
                  className={shown.includes(index)
                    ? 'block overflow-hidden rounded-field ring-2 ring-primary focus-visible:-outline-offset-2'
                    : 'block overflow-hidden rounded-field opacity-70 ring-1 ring-base-content/10 motion-safe:transition hover:opacity-100 focus-visible:opacity-100 focus-visible:-outline-offset-2'}
                >
                  <img src={mangaUrl.strip(volumeId, index)} alt="" loading="lazy" decoding="async" draggable={false} className="h-24 w-auto min-w-16 bg-base-300 object-cover" />
                  <span className="block bg-base-200 py-0.5 text-center text-[10px] tabular-nums text-base-content/70">{index + 1}</span>
                </button>
              </li>
            )) : null}
          </ol>
        </div>
      </div>
      <div className="flex h-12 items-center gap-3 px-4">
        <span className="w-20 shrink-0 text-xs tabular-nums text-base-content/70">{pageLabel}</span>
        {rtl ? next : previous}
        <input
          type="range"
          min={0}
          max={Math.max(0, pageCount - 1)}
          value={page}
          onChange={(event) => onSeek(Number(event.target.value))}
          aria-label={t('seek')}
          className="range range-primary range-xs flex-1"
          // A right-to-left book fills from the right, like its pages.
          style={rtl ? { transform: 'scaleX(-1)' } : undefined}
        />
        {rtl ? previous : next}
        <span className="flex w-20 shrink-0 items-center justify-end gap-1 text-xs text-base-content/55">
          <ArrowLeftRight className="h-3 w-3" aria-hidden="true" />
          {rtl ? t('direction.rtlShort') : t('direction.ltrShort')}
        </span>
      </div>
    </footer>
  );
}

/** Past the last page: what is next, without closing the book under the user. */
export function EndCard({ volumeTitle, next, onNext, onRestart, onClose, onDismiss }: {
  volumeTitle: string;
  next: MangaVolume | null;
  onNext: () => void;
  onRestart: () => void;
  onClose: () => void;
  /** A click beside the card: back to the last page. */
  onDismiss: () => void;
}) {
  const { t } = useTranslation('mangaReader');
  return (
    <div
      className="absolute inset-0 z-20 grid place-items-center bg-neutral/40 p-6 backdrop-blur-[2px]"
      data-reader-chrome
      onClick={(event) => { if (event.target === event.currentTarget) onDismiss(); }}
    >
      <div className="surface-raised manga-pop-in pointer-events-auto w-full max-w-sm p-6 text-center text-base-content" role="dialog" aria-label={t('end.title', { volume: volumeTitle })}>
        <span className="mx-auto grid h-12 w-12 place-items-center rounded-full bg-success/15 text-success">
          <BookOpen className="h-6 w-6" aria-hidden="true" />
        </span>
        <h2 className="mt-4 text-lg font-bold">{t('end.title', { volume: volumeTitle })}</h2>
        <p className="mt-1 text-sm text-base-content/60">{next ? t('end.next', { volume: next.title }) : t('end.last')}</p>
        <div className="mt-5 flex flex-col gap-2">
          {next ? (
            <Button variant="primary" onClick={onNext} autoFocus>
              <SkipForward className="h-4 w-4" />
              {t('end.readNext', { volume: next.title })}
            </Button>
          ) : null}
          <div className="flex gap-2">
            <Button appearance="outline" size="sm" className="flex-1" onClick={onRestart}>{t('end.restart')}</Button>
            <Button appearance="ghost" size="sm" className="flex-1" onClick={onClose}>{t('close')}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
