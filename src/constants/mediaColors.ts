// Copied from NihongoTracker/Frontend/src/constants/mediaColors.ts so both apps
// colour-code the same media types identically.
export const MEDIA_TYPE_COLORS: Record<string, string> = {
  vn: '#3a70e4',
  game: '#59c94e',
  anime: '#26b2f2',
  video: '#2cc9a4',
  'tv show': '#f8b420',
  manga: '#ee4466',
  reading: '#b34ce6',
  movie: '#f77118',
  book: '#7c6cf0',
  audio: '#f2a15a',
  other: '#6b7280',
};

export interface MediaTypeClasses {
  color: string;
  bgColor: string;
  borderColor: string;
  accentColor: string;
}

export const MEDIA_TYPE_CLASSES: Record<string, MediaTypeClasses> = {
  vn: {
    color: 'text-[#3a70e4]',
    bgColor: 'bg-[#3a70e4]/10',
    borderColor: 'border-[#3a70e4]/30',
    accentColor: 'bg-[#3a70e4]',
  },
  game: {
    color: 'text-[#59c94e]',
    bgColor: 'bg-[#59c94e]/10',
    borderColor: 'border-[#59c94e]/30',
    accentColor: 'bg-[#59c94e]',
  },
  anime: {
    color: 'text-[#26b2f2]',
    bgColor: 'bg-[#26b2f2]/10',
    borderColor: 'border-[#26b2f2]/30',
    accentColor: 'bg-[#26b2f2]',
  },
  video: {
    color: 'text-[#2cc9a4]',
    bgColor: 'bg-[#2cc9a4]/10',
    borderColor: 'border-[#2cc9a4]/30',
    accentColor: 'bg-[#2cc9a4]',
  },
  'tv show': {
    color: 'text-[#f8b420]',
    bgColor: 'bg-[#f8b420]/10',
    borderColor: 'border-[#f8b420]/30',
    accentColor: 'bg-[#f8b420]',
  },
  manga: {
    color: 'text-[#ee4466]',
    bgColor: 'bg-[#ee4466]/10',
    borderColor: 'border-[#ee4466]/30',
    accentColor: 'bg-[#ee4466]',
  },
  reading: {
    color: 'text-[#b34ce6]',
    bgColor: 'bg-[#b34ce6]/10',
    borderColor: 'border-[#b34ce6]/30',
    accentColor: 'bg-[#b34ce6]',
  },
  movie: {
    color: 'text-[#f77118]',
    bgColor: 'bg-[#f77118]/10',
    borderColor: 'border-[#f77118]/30',
    accentColor: 'bg-[#f77118]',
  },
  book: {
    color: 'text-[#7c6cf0]',
    bgColor: 'bg-[#7c6cf0]/10',
    borderColor: 'border-[#7c6cf0]/30',
    accentColor: 'bg-[#7c6cf0]',
  },
  audio: {
    color: 'text-[#f2a15a]',
    bgColor: 'bg-[#f2a15a]/10',
    borderColor: 'border-[#f2a15a]/30',
    accentColor: 'bg-[#f2a15a]',
  },
  other: {
    color: 'text-[#6b7280]',
    bgColor: 'bg-[#6b7280]/10',
    borderColor: 'border-[#6b7280]/30',
    accentColor: 'bg-[#6b7280]',
  },
};

export function getMediaTypeColor(type?: string | null): string {
  return MEDIA_TYPE_COLORS[type ?? 'other'] ?? MEDIA_TYPE_COLORS.other;
}

export function getMediaTypeClasses(type?: string | null): MediaTypeClasses {
  return MEDIA_TYPE_CLASSES[type ?? 'other'] ?? MEDIA_TYPE_CLASSES.other;
}
