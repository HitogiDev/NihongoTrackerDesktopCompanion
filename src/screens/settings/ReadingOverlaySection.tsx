import { useTranslation } from 'react-i18next';
import { ExternalLink } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { ipc } from '../../lib/ipc';

/** The overlay's own settings live in a window over the game, where their
 *  effect can be seen; this section only explains and opens it. */
export function ReadingOverlaySection() {
  const { t } = useTranslation('readingOverlay');
  return (
    <div className="surface space-y-3 p-4">
      <p className="text-sm text-base-content/70">{t('settingsHint')}</p>
      <Button appearance="outline" onClick={() => void ipc.openAuxWindow('reading-overlay-settings')}>
        <ExternalLink className="h-4 w-4" />
        {t('openSettings')}
      </Button>
    </div>
  );
}
