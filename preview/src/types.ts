export type SourceKind = 'steam' | 'gog_galaxy' | 'steam_emulator' | 'green_luma' | 'rpcs3' | 'epic' | 'gog' | 'luma_play' | 'watchdog_cache';
export type NotificationMode = 'overlay_with_native_fallback' | 'overlay_only' | 'native_only';

export interface SourceLocation {
  id: string;
  kind: SourceKind;
  path: string;
  enabled: boolean;
  notify: boolean;
}

export interface AppSettings {
  language: string;
  username: string;
  usernameCustomized?: boolean;
  profileAvatarPath?: string;
  profileAvatarSquared: boolean;
  thumbnailPortrait: boolean;
  showHidden: boolean;
  mergeDuplicate: boolean;
  timeMergeRecentFirst: boolean;
  hideZero: boolean;
  runAtLogin: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  checkForUpdates: boolean;
  skippedUpdateVersion?: string;
  blacklistedGameIds: string[];
  gameLaunchConfigs: Record<string, { executable: string; arguments: string }>;
  showPlayButton: boolean;
  notificationMode: NotificationMode;
  notificationEnabled: boolean;
  notifyOnProgress: boolean;
  notifyOnPlaytime: boolean;
  notificationShowDescription: boolean;
  notificationMaxAgeSeconds: number;
  notificationRequireRunningGame: boolean;
  notificationPreset: string;
  notificationSound: string;
  notificationCustomSoundPath?: string;
  rumbleEnabled: boolean;
  rumbleStrengthPercent: number;
  rumbleDurationMs: number;
  screenshotEnabled: boolean;
  screenshotOverwrite: boolean;
  obsReplayEnabled: boolean;
  obsHost: string;
  obsPort: number;
  obsPassword: string;
  obsStartReplayBuffer: boolean;
  clipDirectory?: string;
  customActionEnabled: boolean;
  customActionExecutable: string;
  customActionArguments: string;
  customActionWorkingDirectory?: string;
  customActionHideWindow: boolean;
  notificationDurationPercent: number;
  notificationScalePercent: number;
  gameBarEnabled: boolean;
  gameBarFullscreenOnly: boolean;
  gameBarToken: string;
  achievementOverlayEnabled: boolean;
  achievementOverlayHotkey: string;
  achievementOverlayScalePercent: number;
  websocketEnabled: boolean;
  websocketHost: string;
  websocketPort: number;
  gntpEnabled: boolean;
  gntpHost: string;
  gntpPort: number;
  sourceLocations: SourceLocation[];
  sourcesInitialized: boolean;
  showCachedGames: boolean;
  notificationPosition: string;
  screenshotDirectory?: string;
  steamEnabled: boolean;
  steamLibraryMode: string;
  steamEmulatorEnabled: boolean;
  greenLumaEnabled: boolean;
  rpcs3Enabled: boolean;
  epicEnabled: boolean;
  gogEnabled: boolean;
  gogGalaxyEnabled: boolean;
  lumaPlayEnabled: boolean;
  watchdogCacheEnabled: boolean;
  steamPublicFallback: boolean;
  steamAccountId?: string;
  steamApiKey: string;
}

export interface UpdateInfo {
  version: string;
  releaseUrl: string;
  installerName: string;
}

export interface SettingsApplyResult {
  libraryChanged: boolean;
  scanRequired: boolean;
}

export interface OperationSnapshot {
  kind?: 'scan' | 'metadata';
  message: string;
  completed: number;
  total: number;
  startedAt?: number;
  finishedAt?: number;
  lastSuccessAt?: number;
  lastError?: string;
}

export interface GameSummary {
  sourceId: string;
  sourceKind?: SourceKind;
  gameId: string;
  name: string;
  unlocked: number;
  total: number;
  platinum?: number;
  gold?: number;
  silver?: number;
  bronze?: number;
  lastUnlockTime: number;
  playtimeSeconds: number;
  lastPlayed: number;
  icon?: string;
  tracked: boolean;
}

export interface AchievementObservation {
  sourceId: string;
  originSourceId?: string;
  gameId: string;
  achievementId: string;
  achieved: boolean;
  hidden: boolean;
  globalPercentHundredths?: number;
  trophyGrade?: string;
  currentProgress: number;
  maxProgress: number;
  unlockTime: number;
  displayName?: string;
  description?: string;
  icon?: string;
}

export interface NotificationEvent {
  id: number;
  eventKey: string;
  kind: 'unlock' | 'progress';
  observation: AchievementObservation;
  attempts: number;
  nextAttemptAt: number;
}

export interface NotificationPresentationSettings {
  mode: NotificationMode;
  showDescription: boolean;
  preset: string;
  sound: string;
  customSoundPath?: string;
  durationPercent: number;
  scalePercent: number;
  position: string;
}

export interface NotificationRenderRequest {
  event: NotificationEvent;
  presentation: NotificationPresentationSettings;
  presetConfig: { width: number; height: number; durationMs: number };
}
