import { useTranslation } from 'react-i18next';
import { BackupSettings } from '../../components/BackupSettings';
import { UpdateSettings } from '../../components/UpdateSettings';
import { SectionTitle } from '../../components/ui/Page';

export function BackupSection() {
  const { t } = useTranslation(['settings', 'common']);
  return (
    <>
      <SectionTitle>{t('common:update.title')}</SectionTitle>
      <UpdateSettings />
      <SectionTitle>{t('backup.section')}</SectionTitle>
      <BackupSettings />
    </>
  );
}
