import type { AppSettings, NotificationPresentationSettings } from './types';

export function cloneSettings(settings: AppSettings): AppSettings {
  return structuredClone(settings);
}

export function settingsChanged(current: AppSettings | null, saved: AppSettings | null): boolean {
  return Boolean(current && saved && JSON.stringify(current) !== JSON.stringify(saved));
}

export function notificationPresentation(settings: AppSettings): NotificationPresentationSettings {
  return {
    mode: settings.notificationMode,
    showDescription: settings.notificationShowDescription,
    preset: settings.notificationPreset,
    sound: settings.notificationSound,
    customSoundPath: settings.notificationCustomSoundPath,
    durationPercent: settings.notificationDurationPercent,
    scalePercent: settings.notificationScalePercent,
    position: settings.notificationPosition,
  };
}
