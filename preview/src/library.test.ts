import { describe, expect, it } from 'vitest';
import { completionPercent, sourceDescription, sourceLabel } from './library';

describe('library presentation', () => {
  it('bounds completion values and handles empty games', () => {
    expect(completionPercent({ sourceId: 's', sourceKind: 'steam_emulator', gameId: 'g', name: 'G', unlocked: 0, total: 0, lastUnlockTime: 0, playtimeSeconds: 0, lastPlayed: 0, tracked: true })).toBe(0);
    expect(completionPercent({ sourceId: 's', sourceKind: 'steam_emulator', gameId: 'g', name: 'G', unlocked: 3, total: 2, lastUnlockTime: 0, playtimeSeconds: 0, lastPlayed: 0, tracked: true })).toBe(100);
  });

  it('formats machine-readable source names', () => {
    expect(sourceLabel('steam_emulator')).toBe('Steam emulator save');
    expect(sourceDescription('steam_emulator')).toContain('local Steam emulator save file');
  });
});
