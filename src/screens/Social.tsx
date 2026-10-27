import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { Trophy } from 'lucide-react';
import { Page } from '../components/ui/Page';
import { SocialActivityFeed } from '../components/SocialActivityFeed';

export default function SocialScreen() {
  const { t } = useTranslation('social');

  return (
    <Page
      title={t('title')}
      description={t('subtitle')}
      actions={
        <Link to="/ranking" className="btn btn-ghost btn-sm">
          <Trophy className="h-4 w-4" />{t('ranking')}
        </Link>
      }
    >
      <SocialActivityFeed />
    </Page>
  );
}
