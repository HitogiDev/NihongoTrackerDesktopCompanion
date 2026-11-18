import { describe, expect, it } from 'vitest';
import { menuGroups } from './LibraryMenu';

describe('library settings menu', () => {
  it('offers only what makes sense on each tab', () => {
    expect(menuGroups('all')).toEqual(['local', 'webdav', 'extensions']);
    expect(menuGroups('local')).toEqual(['local']);
    expect(menuGroups('webdav')).toEqual(['webdav']);
    expect(menuGroups('extensions')).toEqual(['extensions']);
  });
});
