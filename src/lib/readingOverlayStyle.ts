import type { CSSProperties } from 'react';
import type { ReadingOverlaySettings } from './types';

export function readingOverlayTextStyle(preferences?: ReadingOverlaySettings): CSSProperties {
  const shadowOpacity = preferences?.text_shadow_opacity ?? 0;
  return {
    fontSize: preferences?.font_size ?? 24,
    fontFamily: preferences?.font_family,
    fontWeight: preferences?.font_weight ?? 400,
    WebkitTextStroke: `${preferences?.text_outline_width ?? 0}px ${preferences?.text_outline_color ?? '#ffffff'}`,
    paintOrder: 'stroke fill',
    textShadow: shadowOpacity > 0
      ? `0 2px ${preferences?.text_shadow_blur ?? 6}px color-mix(in srgb, ${preferences?.text_shadow_color ?? '#000000'} ${shadowOpacity}%, transparent)`
      : 'none',
  };
}
