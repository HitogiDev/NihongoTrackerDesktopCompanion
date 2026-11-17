// Same options and stacks as NihongoTracker/Frontend/src/screens/HookerScreen.tsx.

export const LOCAL_SESSION = 'local';

export const HOOKER_THEME_OPTIONS: Array<{ value: string; label?: string; labelKey?: string }> = [
  { value: '', labelKey: 'settings.useAppTheme' },
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'cupcake', label: 'Cupcake' },
  { value: 'bumblebee', label: 'Bumblebee' },
  { value: 'emerald', label: 'Emerald' },
  { value: 'corporate', label: 'Corporate' },
  { value: 'synthwave', label: 'Synthwave' },
  { value: 'retro', label: 'Retro' },
  { value: 'valentine', label: 'Valentine' },
  { value: 'halloween', label: 'Halloween' },
  { value: 'garden', label: 'Garden' },
  { value: 'forest', label: 'Forest' },
  { value: 'dracula', label: 'Dracula' },
  { value: 'cmyk', label: 'CMYK' },
  { value: 'night', label: 'Night' },
  { value: 'winter', label: 'Winter' },
  { value: 'dim', label: 'Dim' },
  { value: 'sunset', label: 'Sunset' },
  { value: 'abyss', label: 'Abyss' },
];

export const FONT_FAMILY_OPTIONS = [
  { value: 'Noto Sans JP', label: 'Noto Sans JP' },
  { value: 'Noto Serif JP', label: 'Noto Serif JP' },
  { value: 'Sawarabi Gothic', label: 'Sawarabi Gothic' },
  { value: 'Ackaisyo', label: 'Ackaisyo' },
  { value: 'Hyoukinna Pop', label: 'Hyoukinna Pop' },
  { value: 'Humour Original', label: 'Humour Original' },
  { value: 'sans-serif', label: 'Sans Serif' },
];

export const FONT_FAMILY_STACKS: Record<string, string> = {
  'Noto Sans JP': '"Noto Sans JP Local", "Noto Sans JP", "Noto Sans CJK JP", sans-serif',
  'Noto Serif JP': '"Noto Serif JP Local", "Noto Serif JP", serif',
  'Sawarabi Gothic': '"Sawarabi Gothic", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  Ackaisyo: '"Ackaisyo", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'Hyoukinna Pop': '"Hyoukinna Pop", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'Humour Original': '"Humour Original", "Noto Sans JP Local", "Noto Sans JP", sans-serif',
  'sans-serif': 'sans-serif',
};

export const COLUMN_HEIGHT = 'calc(100vh - 160px)';
