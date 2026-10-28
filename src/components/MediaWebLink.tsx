import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useAppStore } from '../store/useAppStore';

// Log types that have a media page on the site, which lives at
// `/:mediaType/:mediaId` with the same spelling (`tv show`, with a space).
// `reading`, `audio` and `other` logs can point at several media types, so
// there is no page to guess.
const MEDIA_PAGE_TYPES = new Set(['anime', 'manga', 'light-novel', 'vn', 'video', 'movie', 'tv show', 'game', 'book']);

export function mediaWebUrl(baseUrl: string | undefined, type: string | undefined, mediaId: string | undefined) {
  if (!baseUrl || !type || !mediaId || !MEDIA_PAGE_TYPES.has(type)) return null;
  return `${baseUrl.replace(/\/+$/, '')}/${encodeURIComponent(type)}/${encodeURIComponent(mediaId)}`;
}

/**
 * A media title that opens its NihongoTracker page in the browser. Unlinked
 * logs (no `mediaId`) and types without a page stay plain text.
 */
export function MediaWebLink({
  type,
  mediaId,
  className = '',
  children,
}: {
  type?: string;
  mediaId?: string;
  className?: string;
  children: ReactNode;
}) {
  const { t } = useTranslation('common');
  const baseUrl = useAppStore((s) => s.settings?.api_base_url);
  const url = mediaWebUrl(baseUrl, type, mediaId);
  if (!url) return <span className={className}>{children}</span>;
  return (
    <a
      href={url}
      title={t('openInWeb')}
      className={`link link-hover hover:text-primary ${className}`}
      // The webview must not navigate itself; the page opens in the browser.
      onClick={(event) => {
        event.preventDefault();
        void openUrl(url).catch(() => undefined);
      }}
    >
      {children}
    </a>
  );
}
