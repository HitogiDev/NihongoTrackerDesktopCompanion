import { DiscordSettings } from '../../components/DiscordSettings';
import { useAppStore } from '../../store/useAppStore';
import type { Settings } from '../../lib/types';

export function DiscordSection({ settings }: { settings: Settings }) {
  const updateSettings = useAppStore((s) => s.updateSettings);
  return <DiscordSettings settings={settings} onChange={(patch) => void updateSettings(patch)} />;
}
