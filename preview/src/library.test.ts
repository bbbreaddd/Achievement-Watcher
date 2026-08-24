import { describe, expect, it } from 'vitest';
import { completionPercent, preferredAchievementSource, sourceDescription, sourceLabel } from './library';

describe('library presentation', () => {
  it('bounds completion values and handles empty games', () => {
    expect(completionPercent({ sourceId: 's', sourceKind: 'steam_emulator', gameId: 'g', name: 'G', unlocked: 0, total: 0, lastUnlockTime: 0, playtimeSeconds: 0, lastPlayed: 0, tracked: true })).toBe(0);
    expect(completionPercent({ sourceId: 's', sourceKind: 'steam_emulator', gameId: 'g', name: 'G', unlocked: 3, total: 2, lastUnlockTime: 0, playtimeSeconds: 0, lastPlayed: 0, tracked: true })).toBe(100);
  });

  it('formats machine-readable source names', () => {
    expect(sourceLabel('steam_emulator')).toBe('Steam emulator save');
    expect(sourceDescription('steam_emulator')).toContain('local Steam emulator save file');
  });

  it('prefers Steam client progress over merged and emulator progress', () => {
    expect(preferredAchievementSource([
      { sourceId: 'merged', sourceKind: 'steam' },
      { sourceId: 'emulator', sourceKind: 'steam_emulator' },
      { sourceId: 'client', sourceKind: 'steam' },
    ], 'merged')).toBe('client');
  });

  it('prefers GOG Galaxy progress over merged and emulator progress', () => {
    expect(preferredAchievementSource([
      { sourceId: 'merged', sourceKind: 'gog_galaxy' },
      { sourceId: 'emulator', sourceKind: 'gog' },
      { sourceId: 'galaxy', sourceKind: 'gog_galaxy' },
    ], 'merged')).toBe('galaxy');
  });
});
