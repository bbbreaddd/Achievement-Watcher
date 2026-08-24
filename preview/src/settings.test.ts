import { describe, expect, it } from 'vitest';
import { cloneSettings, notificationPresentation, settingsChanged } from './settings';
import type { AppSettings } from './types';

const settings = {
  language: 'english',
  notificationMode: 'overlay_only',
  notificationShowDescription: true,
  notificationPreset: 'steam',
  notificationSound: 'steam_deck',
  notificationDurationPercent: 100,
  notificationScalePercent: 90,
  notificationPosition: 'bottom_right',
  sourceLocations: [],
} as unknown as AppSettings;

describe('settings drafts', () => {
  it('clones nested values without changing the saved settings', () => {
    const draft = cloneSettings(settings);
    draft.sourceLocations.push({ id: 'one', kind: 'steam', path: 'C:\\Steam', enabled: true, notify: true });
    expect(settings.sourceLocations).toEqual([]);
    expect(settingsChanged(draft, settings)).toBe(true);
  });

  it('creates a renderer-only notification preview', () => {
    expect(notificationPresentation(settings)).toEqual({
      mode: 'overlay_only',
      showDescription: true,
      preset: 'steam',
      sound: 'steam_deck',
      customSoundPath: undefined,
      durationPercent: 100,
      scalePercent: 90,
      position: 'bottom_right',
    });
  });
});
