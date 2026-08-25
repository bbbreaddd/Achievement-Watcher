<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount, tick } from 'svelte';
  import { completionPercent, preferredAchievementSource, sourceDescription, sourceLabel } from './library';
  import { operationMessage } from './operation';
  import { notificationStatusMessage } from './notification-status';
  import { cloneSettings, notificationPresentation, settingsChanged } from './settings';
  import defaultAvatar from '../../app/resources/img/avatar.png';
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import GameConfigDialog from './components/GameConfigDialog.svelte';
  import SourceBadge from './components/SourceBadge.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import TitleBar from './components/TitleBar.svelte';
  import type { AchievementObservation, AppSettings, GameSummary, OperationSnapshot, SettingsApplyResult, SourceKind, UpdateInfo } from './types';

  let games: GameSummary[] = [];
  let settings: AppSettings | null = null;
  let settingsSnapshot: AppSettings | null = null;
  let status = 'Preparing local library…';
  let startupError = '';
  let liveUpdateErrors: string[] = [];
  let settingsError = '';
  let initializing = true;
  let scanning = false;
  let operation: OperationSnapshot | null = null;
  let view: 'library' | 'settings' = 'library';
  let selectedGame: GameSummary | null = null;
  let achievements: AchievementObservation[] = [];
  let achievementStatus = '';
  let query = '';
  let libraryFilter: 'all' | 'tracked' | 'cached' = 'all';
  let librarySort: 'name' | 'progress' | 'recent' = 'name';
  let settingsTab: 'general' | 'notification' | 'souvenir' | 'folder' | 'source' | 'advanced' | 'debug' = 'general';
  type SourceChoice = { sourceId: string; sourceKind?: SourceKind; sourcePath?: string };
  let gameMenu: { game: GameSummary; x: number; y: number; sources?: SourceChoice[]; sourceError?: string } | null = null;
  let avatarMenu: { x: number; y: number } | null = null;
  let achievementSort: 'name' | 'time' | 'progress' | 'rarity' = 'rarity';
  let achievementQuery = '';
  let unlockedCollapsed = false;
  let lockedCollapsed = false;
  let hiddenCollapsed = false;
  let steamAccounts: Array<{ accountId: string; steamId: string; name: string; mostRecent: boolean; avatarPath?: string }> = [];
  let gameConfig: { game: GameSummary; executable: string; arguments: string } | null = null;
  let highlightedAchievement = '';
  let revealHiddenForGame = false;
  let avatarData = '';
  let gameSourceChoices: SourceChoice[] = [];
  let activeAchievementSource = '';
  let achievementRequest = 0;
  let diagnosticData: { appVersion: string; observationCount: number; gameCount: number; enabledSourceCount: number; missingSourceCount: number; pendingNotifications: number; failedNotifications: number; recentErrors: string[]; notificationLog: string; watchers: Array<{ name: string; enabled: boolean; lastHeartbeatAt: number; lastWorkAt?: number; lastSuccessAt?: number; lastError?: string }> } | null = null;
  let availableUpdate: UpdateInfo | null = null;
  let installingUpdate = false;
  let blacklistedGames: Array<{ gameId: string; name: string }> = [];
  let defaultScreenshotDirectory = '';
  let defaultClipDirectory = '';
  let savingSettings = false;
  let maximized = false;
  let gameMenuElement: HTMLElement;
  let avatarMenuElement: HTMLElement;
  let menuReturnFocus: HTMLElement | null = null;
  let gameConfigReturnFocus: HTMLElement | null = null;
  let confirmationReturnFocus: HTMLElement | null = null;
  let confirmationBusy = false;
  let confirmation: { title: string; message: string; confirmLabel: string; action: () => void | Promise<void> } | null = null;
  const appWindow = getCurrentWindow();
  const localeModules = import.meta.glob('../../app/locale/lang/english.json', { eager: true, import: 'default' }) as Record<string, Record<string, unknown>>;
  let locale: Record<string, unknown> = localeModules['../../app/locale/lang/english.json'] ?? {};

  function t(path: string, fallback: string) {
    let value: unknown = locale;
    for (const part of path.split('.')) value = typeof value === 'object' && value ? (value as Record<string, unknown>)[part] : undefined;
    return typeof value === 'string' ? value : fallback;
  }

  function applyLanguage() {
    locale = localeModules['../../app/locale/lang/english.json'] ?? {};
    document.documentElement.lang = 'en';
  }

  async function runWindowAction(label: string, action: () => Promise<void>) {
    try {
      await action();
    } catch (error) {
      status = `Could not ${label} the window: ${String(error)}`;
    }
  }

  async function toggleMaximize() {
    await runWindowAction('resize', () => appWindow.toggleMaximize());
    maximized = await appWindow.isMaximized().catch(() => maximized);
  }

  function closeWindow() {
    if (savingSettings) {
      status = 'Wait for settings to finish saving before closing the window';
      return;
    }
    const close = () => runWindowAction('close', () => appWindow.close());
    if (view === 'settings' && settingsDirty()) {
      requestConfirmation(
        'Discard settings changes?',
        'Your unsaved settings will be discarded before the window closes.',
        'Discard and close',
        async () => {
          await cancelSettings();
          await close();
        },
      );
      return;
    }
    void close();
  }

  function hasSteamAppId(game: GameSummary) {
    return /^\d+$/.test(game.gameId)
      && ['steam', 'steam_emulator', 'green_luma', 'watchdog_cache'].includes(game.sourceKind ?? '');
  }

  function imageUrl(value: string) {
    return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') ? convertFileSrc(value) : value;
  }

  function gameArtwork(game: GameSummary) {
    if (game.icon && (/^[a-zA-Z]:[\\/]/.test(game.icon) || game.icon.startsWith('\\\\'))) {
      return imageUrl(game.icon);
    }
    if (settings?.thumbnailPortrait && hasSteamAppId(game)) {
      return `https://cdn.cloudflare.steamstatic.com/steam/apps/${game.gameId}/library_600x900_2x.jpg`;
    }
    return game.icon ? imageUrl(game.icon) : undefined;
  }

  function gameArtworkFailed(event: Event, game: GameSummary) {
    const image = event.currentTarget as HTMLImageElement;
    const localFailed = game.icon && (/^[a-zA-Z]:[\\/]/.test(game.icon) || game.icon.startsWith('\\\\')) && image.src === imageUrl(game.icon);
    if (localFailed && settings?.thumbnailPortrait && hasSteamAppId(game)) image.src = `https://cdn.cloudflare.steamstatic.com/steam/apps/${game.gameId}/library_600x900_2x.jpg`;
    else if (settings?.thumbnailPortrait && game.icon && image.src !== imageUrl(game.icon)) image.src = imageUrl(game.icon);
    else image.remove();
  }

  function visibleGames() {
    const term = query.trim().toLowerCase();
    return games.filter((game) => (!term || game.name.toLowerCase().includes(term) || game.gameId.includes(term))
      && (libraryFilter === 'all' || (libraryFilter === 'tracked' ? game.tracked : !game.tracked)))
      .slice().sort((a, b) => {
        if (librarySort === 'progress') return completionPercent(b) - completionPercent(a) || a.name.localeCompare(b.name);
        if (librarySort === 'recent') return b.lastUnlockTime - a.lastUnlockTime || a.name.localeCompare(b.name);
        return a.name.localeCompare(b.name);
      });
  }

  function clearLibraryFilters() {
    query = '';
    libraryFilter = 'all';
  }

  function fitMenuPosition(element: HTMLElement, x: number, y: number) {
    const bounds = element.getBoundingClientRect();
    return {
      x: Math.max(8, Math.min(x, innerWidth - bounds.width - 8)),
      y: Math.max(8, Math.min(y, innerHeight - bounds.height - 8)),
    };
  }

  async function showGameMenu(event: MouseEvent, game: GameSummary) {
    event.preventDefault();
    event.stopPropagation();
    menuReturnFocus = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    gameMenu = { game, x: event.clientX, y: event.clientY };
    await tick();
    gameMenu = { ...gameMenu, ...fitMenuPosition(gameMenuElement, event.clientX, event.clientY) };
    gameMenuElement?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    try {
      const sources = await invoke<SourceChoice[]>('openable_game_sources', { gameId: game.gameId });
      if (gameMenu?.game.sourceId === game.sourceId && gameMenu.game.gameId === game.gameId) {
        gameMenu.sources = sources;
      }
    } catch (error) {
      if (gameMenu?.game.sourceId === game.sourceId && gameMenu.game.gameId === game.gameId) {
        gameMenu.sources = [];
        gameMenu.sourceError = String(error);
      }
    }
  }

  function closeGameMenu(restoreFocus = false) {
    if (!gameMenu) return;
    gameMenu = null;
    const target = menuReturnFocus;
    menuReturnFocus = null;
    if (restoreFocus) {
      void tick().then(() => target?.focus());
    }
  }

  function handleMenuKeydown(event: KeyboardEvent, element: HTMLElement, close: () => void) {
    const items = Array.from(element.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
    if (!items.length) return;
    const current = Math.max(0, items.indexOf(document.activeElement as HTMLButtonElement));
    let target: HTMLButtonElement | undefined;
    if (event.key === 'ArrowDown') target = items[(current + 1) % items.length];
    else if (event.key === 'ArrowUp') target = items[(current - 1 + items.length) % items.length];
    else if (event.key === 'Home') target = items[0];
    else if (event.key === 'End') target = items.at(-1);
    else if (event.key === 'Escape') {
      event.preventDefault();
      close();
      return;
    } else if (event.key === 'Tab') {
      close();
      return;
    }
    if (target) {
      event.preventDefault();
      target.focus();
    }
  }

  async function showAvatarMenu(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    menuReturnFocus = event.currentTarget instanceof HTMLElement ? event.currentTarget : null;
    avatarMenu = { x: event.clientX, y: event.clientY };
    await tick();
    avatarMenu = fitMenuPosition(avatarMenuElement, event.clientX, event.clientY);
    avatarMenuElement?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }

  function closeAvatarMenu(restoreFocus = false) {
    if (!avatarMenu) return;
    avatarMenu = null;
    const target = menuReturnFocus;
    menuReturnFocus = null;
    if (restoreFocus) {
      void tick().then(() => target?.focus());
    }
  }

  function requestConfirmation(title: string, message: string, confirmLabel: string, action: () => void | Promise<void>) {
    confirmationReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    confirmation = { title, message, confirmLabel, action };
  }

  function cancelConfirmation() {
    if (confirmationBusy) return;
    confirmation = null;
    const target = confirmationReturnFocus;
    confirmationReturnFocus = null;
    void tick().then(() => target?.focus());
  }

  async function runConfirmedAction() {
    if (!confirmation || confirmationBusy) return;
    confirmationBusy = true;
    try {
      await confirmation.action();
      confirmation = null;
    } finally {
      confirmationBusy = false;
      confirmationReturnFocus = null;
    }
  }

  function visibleAchievementCount() {
    return achievementRows(true).length
      + achievementRows(false).length
      + ((settings?.showHidden || revealHiddenForGame) ? achievementRows(false, true).length : 0);
  }

  function totalUnlocked() {
    return games.reduce((total, game) => total + game.unlocked, 0);
  }

  function completedGames() {
    return games.filter((game) => game.total > 0 && game.unlocked === game.total).length;
  }

  function gamesWithAchievements() {
    return games.filter((game) => game.total > 0).length;
  }

  function averageCompletion() {
    const tracked = games.filter((game) => game.tracked && game.total > 0);
    return tracked.length ? Math.floor(tracked.reduce((total, game) => total + completionPercent(game), 0) / tracked.length) : 0;
  }

  function trophyTotal(grade: 'platinum' | 'gold' | 'silver' | 'bronze') {
    return games.reduce((total, game) => total + (game[grade] ?? 0), 0);
  }

  function achievementTrophyTotal(grade: 'platinum' | 'gold' | 'silver' | 'bronze') {
    return achievements.filter((achievement) => achievement.trophyGrade === grade && achievement.achieved).length;
  }

  function detailUnlocked() { return achievements.filter((achievement) => achievement.achieved).length; }

  function activeSteamAccount() {
    return steamAccounts.find((account) => account.accountId === settings?.steamAccountId || account.steamId === settings?.steamAccountId)
      ?? steamAccounts.find((account) => account.mostRecent)
      ?? steamAccounts[0];
  }

  async function useDefaultSteamName() {
    const account = activeSteamAccount();
    if (!settings || settings.usernameCustomized !== false || !account?.name || settings.username === account.name) return;
    const previousName = settings.username;
    settings.username = account.name;
    if (!(await save())) settings.username = previousName;
  }

  async function restoreSteamName() {
    const account = activeSteamAccount();
    if (!settings || !account?.name) return;
    settings.username = account.name;
    settings.usernameCustomized = false;
    await save();
  }

  function achievementRows(achieved: boolean, hiddenOnly = false) {
    const term = achievementQuery.trim().toLowerCase();
    return achievements.filter((achievement) => achievement.achieved === achieved
      && (achieved || achievement.hidden === hiddenOnly)
      && (!term
      || (achievement.displayName ?? achievement.achievementId).toLowerCase().includes(term)
      || (achievement.description ?? '').toLowerCase().includes(term)))
      .slice().sort((left, right) => {
        if (achievementSort === 'time') return right.unlockTime - left.unlockTime
          || (left.displayName ?? left.achievementId).localeCompare(right.displayName ?? right.achievementId);
        if (achievementSort === 'progress') {
          const leftProgress = left.maxProgress > 0 ? left.currentProgress / left.maxProgress : Number(left.achieved);
          const rightProgress = right.maxProgress > 0 ? right.currentProgress / right.maxProgress : Number(right.achieved);
          return rightProgress - leftProgress || (left.displayName ?? left.achievementId).localeCompare(right.displayName ?? right.achievementId);
        }
        if (achievementSort === 'rarity') return (left.globalPercentHundredths ?? 10_001) - (right.globalPercentHundredths ?? 10_001)
          || (left.displayName ?? left.achievementId).localeCompare(right.displayName ?? right.achievementId);
        return (left.displayName ?? left.achievementId).localeCompare(right.displayName ?? right.achievementId);
      });
  }

  function hiddenLockedCount() {
    return achievements.filter((achievement) => !achievement.achieved && achievement.hidden).length;
  }

  function formatUnlockTime(timestamp: number) {
    const date = new Date(timestamp * 1000);
    const deltaSeconds = Math.round((date.getTime() - Date.now()) / 1000);
    const absolute = Math.abs(deltaSeconds);
    const [value, unit] = absolute >= 86_400
      ? [Math.round(deltaSeconds / 86_400), 'day']
      : absolute >= 3_600
        ? [Math.round(deltaSeconds / 3_600), 'hour']
        : absolute >= 60
          ? [Math.round(deltaSeconds / 60), 'minute']
          : [deltaSeconds, 'second'];
    const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' }).format(value, unit as Intl.RelativeTimeFormatUnit);
    return `${date.toLocaleString()} • ${relative}`;
  }

  async function openGame(game: GameSummary, achievementId = '') {
    const request = ++achievementRequest;
    gameMenu = null;
    achievementQuery = '';
    unlockedCollapsed = false;
    lockedCollapsed = false;
    hiddenCollapsed = false;
    highlightedAchievement = achievementId;
    revealHiddenForGame = false;
    selectedGame = game;
    achievements = [];
    achievementStatus = 'Loading achievements…';
    try {
      const sourceChoices = await invoke<SourceChoice[]>('game_sources', { gameId: game.gameId });
      if (request !== achievementRequest) return;
      gameSourceChoices = sourceChoices;
      const sourceId = preferredAchievementSource(sourceChoices, game.sourceId);
      activeAchievementSource = sourceId;
      let loadedAchievements = await invoke<AchievementObservation[]>('list_achievements', {
        sourceId,
        gameId: game.gameId,
      });
      if (request !== achievementRequest) return;
      let metadataError = '';
      if (loadedAchievements.length === 0 && hasSteamAppId(game)) {
        achievementStatus = 'Fetching Steam achievement information…';
        try {
          await invoke('refresh_metadata', { gameId: game.gameId });
          loadedAchievements = await invoke<AchievementObservation[]>('list_achievements', {
            sourceId,
            gameId: game.gameId,
          });
        } catch (error) {
          metadataError = String(error);
        }
      }
      if (request !== achievementRequest) return;
      achievements = loadedAchievements;
      achievementStatus = achievements.length === 0
        ? `No achievements were read from this source.${metadataError ? ` Steam information could not be refreshed: ${metadataError}` : ''}`
        : '';
      if (achievementId) {
        await tick();
        document.querySelector(`[data-achievement-id="${CSS.escape(achievementId)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    } catch (error) {
      if (request === achievementRequest) {
        achievementStatus = `Could not load achievements: ${String(error)}`;
      }
    }
  }

  async function changeAchievementSource() {
    if (!selectedGame) return;
    const request = ++achievementRequest;
    const gameId = selectedGame.gameId;
    const sourceId = activeAchievementSource;
    achievementStatus = 'Loading achievements…';
    try {
      const loadedAchievements = await invoke<AchievementObservation[]>('list_achievements', {
        sourceId,
        gameId,
      });
      if (request !== achievementRequest) return;
      achievements = loadedAchievements;
      achievementStatus = achievements.length ? '' : 'No achievements were read from this source.';
    } catch (error) {
      if (request === achievementRequest) {
        achievements = [];
        achievementStatus = `Could not load this source: ${String(error)}`;
      }
    }
  }

  function closeGameDetails() {
    achievementRequest += 1;
    selectedGame = null;
  }

  function kindForSource(sourceId?: string) {
    return gameSourceChoices.find((choice) => choice.sourceId === sourceId)?.sourceKind;
  }

  function activeSourceKind() {
    return kindForSource(activeAchievementSource) ?? selectedGame?.sourceKind;
  }

  function sourceChoiceLabel(choice: SourceChoice) {
    if (choice.sourceId === 'merged') return 'Combined — all enabled progress sources';
    return `${sourceLabel(choice.sourceKind)} — ${sourceDescription(choice.sourceKind)}`;
  }

  async function testNotification() {
    status = 'Sending achievement notification test…';
    try {
      const presentation = settings ? notificationPresentation(settings) : undefined;
      await invoke('test_notification', { presentation });
    } catch (error) {
      status = `Notification test failed: ${String(error)}`;
    }
  }

  async function testNotificationKind(command: 'test_progress_notification' | 'test_playtime_notification', label: string) {
    status = `Sending ${label.toLowerCase()} notification test…`;
    try {
      const presentation = settings ? notificationPresentation(settings) : undefined;
      await invoke(command, { presentation });
    } catch (error) {
      status = `${label} notification test failed: ${String(error)}`;
    }
  }

  async function refresh() {
    games = await invoke<GameSummary[]>('list_games');
  }

  type OpenGameRequest = { sourceId: string; gameId: string; achievementId: string };

  async function consumeOpenGameRequest(fallback?: OpenGameRequest) {
    const pending = await invoke<OpenGameRequest | null>('take_pending_open_game').catch(() => null);
    const request = pending ?? fallback;
    if (!request) return;
    await refresh();
    const game = games.find((item) => item.gameId === request.gameId && item.sourceId === request.sourceId)
      ?? games.find((item) => item.gameId === request.gameId);
    if (game) {
      view = 'library';
      await openGame(game, request.achievementId);
    }
  }

  async function scan(establishBaseline = false) {
    scanning = true;
    status = 'Scanning configured sources…';
    try {
      const count = await invoke<number>('scan_sources', { establishBaseline });
      await refresh();
      status = `Watching ${count} achievement files`;
      return true;
    } catch (error) {
      status = `Scan failed: ${String(error)}`;
      return false;
    } finally {
      scanning = false;
    }
  }

  function displayedStatus() {
    return operationMessage(status, operation, liveUpdateErrors);
  }

  function titleActivity() {
    if (initializing) return 'Loading library…';
    if (installingUpdate) return 'Preparing update…';
    if (savingSettings) return 'Saving settings…';
    if (operation?.kind) return operation.message;
    if (scanning) return 'Scanning achievements…';
    return null;
  }

  async function addSource(kind: SourceKind) {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || !settings) return;
    settings.sourceLocations = [...settings.sourceLocations, {
      id: `${kind}-${Date.now()}`,
      kind,
      path: selected,
      enabled: true,
      notify: true,
    }];
    status = 'Folder added. Save settings to begin watching it.';
  }

  async function persistSettings() {
    if (!settings) return false;
    settingsError = '';
    try {
      const result = await invoke<SettingsApplyResult>('save_settings', { settings });
      if (result.scanRequired) void scan(true);
      else if (result.libraryChanged) await refresh();
      status = 'Settings saved';
      return true;
    } catch (error) {
      settingsError = `Settings were not saved. ${String(error)}`;
      status = 'Could not save settings';
      return false;
    }
  }

  async function save() {
    if (view === 'settings') {
      status = 'Settings have unsaved changes';
      return true;
    }
    return persistSettings();
  }

  function settingsDirty() {
    return settingsChanged(settings, settingsSnapshot);
  }

  async function openSettings() {
    if (!settings) {
      status = 'Settings are unavailable because startup did not finish';
      return;
    }
    settingsSnapshot = cloneSettings(settings);
    settingsError = '';
    settingsTab = 'general';
    view = 'settings';
    blacklistedGames = await invoke<typeof blacklistedGames>('list_blacklisted_games').catch(() =>
      settings?.blacklistedGameIds.map((gameId) => ({ gameId, name: gameId })) ?? []);
  }

  function restoreBlacklistedGame(gameId: string) {
    if (!settings) return;
    settings.blacklistedGameIds = settings.blacklistedGameIds.filter((id) => id !== gameId);
    blacklistedGames = blacklistedGames.filter((game) => game.gameId !== gameId);
    status = 'Game restored. Save settings to return it to the library.';
  }

  async function acceptSettings() {
    if (savingSettings) return;
    savingSettings = true;
    try {
      if (!(await persistSettings())) return;
      if (!settings?.showCachedGames && libraryFilter === 'cached') libraryFilter = 'all';
      settingsSnapshot = null;
      view = 'library';
    } finally {
      savingSettings = false;
    }
  }

  async function cancelSettings() {
    if (settingsSnapshot) {
      settings = cloneSettings(settingsSnapshot);
      applyLanguage();
      await loadAvatar();
    }
    settingsSnapshot = null;
    view = 'library';
    status = 'Settings changes cancelled';
  }

  async function removeSource(id: string) {
    if (!settings) return;
    settings.sourceLocations = settings.sourceLocations.filter((source) => source.id !== id);
    status = 'Folder removed. Save settings to stop watching it.';
  }

  async function detectSources(deep = false, scanAfter = true): Promise<string | null> {
    if (!settings) return 'Settings are not available';
    status = deep ? 'Searching local drives for achievement sources…' : status;
    try {
      const detected = await invoke<AppSettings['sourceLocations']>('detect_sources', { deep });
      const normalizePath = (path: string) => path.replaceAll('\\', '/').toLowerCase();
      const known = new Set(settings.sourceLocations.map((source) => normalizePath(source.path)));
      const additions = detected.filter((source) => !known.has(normalizePath(source.path)));
      settings.sourceLocations = [...settings.sourceLocations, ...additions];
      settings.sourcesInitialized = true;
      if (await save()) {
        status = additions.length ? `Found ${additions.length} achievement folder${additions.length === 1 ? '' : 's'}` : 'No new achievement folders found';
        if (scanAfter && additions.length) await scan(true);
      }
      return null;
    } catch (error) {
      const message = String(error);
      status = `Source discovery failed: ${message}`;
      return message;
    }
  }

  async function chooseScreenshotFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && settings) {
      settings.screenshotDirectory = selected;
      await save();
    }
  }

  async function chooseClipFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && settings) {
      settings.clipDirectory = selected;
      await save();
    }
  }

  async function refreshGameMetadata(game: GameSummary) {
    gameMenu = null;
    status = `Refreshing information for ${game.name}…`;
    try {
      await invoke<number>('refresh_metadata', { gameId: game.gameId });
      await refresh();
      status = `${game.name} information refreshed`;
    } catch (error) {
      status = `Metadata refresh failed: ${String(error)}`;
    }
  }

  async function refreshMissingMetadata(expectedStatus?: string, reportSuccess = true) {
    try {
      const updated = await invoke<number>('refresh_metadata', { gameId: null });
      await refresh();
      if (reportSuccess && (!expectedStatus || status === expectedStatus)) {
        status = updated > 0
          ? `Library information updated for ${updated} item${updated === 1 ? '' : 's'}`
          : 'Library is up to date';
      }
    } catch (error) {
      // Metadata is optional enrichment. A network or Steam Community failure
      // must not turn an otherwise usable local library into a startup failure.
      if (!expectedStatus || status === expectedStatus) {
        const failure = `Some game information could not be refreshed: ${String(error)}`;
        status = expectedStatus ? `${expectedStatus} ${failure}` : failure;
      }
    }
  }

  async function clearGameMetadata(game: GameSummary) {
    gameMenu = null;
    try {
      await invoke('clear_game_metadata', { gameId: game.gameId });
      await refresh();
      status = `Cleared cached information for ${game.name}`;
    } catch (error) {
      status = `Could not clear ${game.name}: ${String(error)}`;
    }
  }

  async function openGameWebsite(game: GameSummary, website: 'steam' | 'steamdb' | 'pcgamingwiki') {
    gameMenu = null;
    try { await invoke('open_game_website', { gameId: game.gameId, website }); }
    catch (error) { status = `Could not open website: ${String(error)}`; }
  }

  async function blacklistGame(game: GameSummary) {
    gameMenu = null;
    if (!settings || settings.blacklistedGameIds.includes(game.gameId)) return;
    const previousIds = settings.blacklistedGameIds;
    settings.blacklistedGameIds = [...settings.blacklistedGameIds, game.gameId];
    if (!(await save())) {
      settings.blacklistedGameIds = previousIds;
      return;
    }
    if (selectedGame?.gameId === game.gameId) closeGameDetails();
  }

  function configureGame(game: GameSummary) {
    gameConfigReturnFocus = menuReturnFocus;
    closeGameMenu();
    const config = settings?.gameLaunchConfigs[game.gameId];
    gameConfig = { game, executable: config?.executable ?? '', arguments: config?.arguments ?? '' };
  }

  function closeGameConfig() {
    gameConfig = null;
    const target = gameConfigReturnFocus;
    gameConfigReturnFocus = null;
    void tick().then(() => target?.focus());
  }

  async function chooseGameExecutable() {
    const selected = await open({ multiple: false, filters: [{ name: 'Executables', extensions: ['exe', 'bat', 'cmd'] }] });
    if (selected && gameConfig) gameConfig.executable = selected;
  }

  async function chooseCustomAction() {
    const selected = await open({ multiple: false, filters: [{ name: 'Programs', extensions: ['exe', 'bat', 'cmd'] }] });
    if (selected && settings) { settings.customActionExecutable = selected; await save(); }
  }

  async function chooseCustomActionDirectory() {
    const selected = await open({ directory: true, multiple: false, title: 'Choose working folder' });
    if (selected && settings) { settings.customActionWorkingDirectory = selected; await save(); }
  }

  async function chooseNotificationSound() {
    const selected = await open({ multiple: false, title: 'Choose notification sound', filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'm4a', 'aac'] }] });
    if (!selected || !settings) return;
    try {
      await invoke<string>('read_notification_audio', { path: selected });
      settings.notificationCustomSoundPath = selected;
      settings.notificationSound = 'custom';
      await save();
      await testNotification();
    } catch (error) {
      status = `Could not use notification sound: ${String(error)}`;
    }
  }

  async function chooseAvatar() {
    const selected = await open({ multiple: false, filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] });
    if (!selected || !settings) return;
    const previousPath = settings.profileAvatarPath;
    const previousData = avatarData;
    try {
      avatarData = await invoke<string>('read_profile_avatar', { path: selected });
      settings.profileAvatarPath = selected;
      if (!(await save())) {
        settings.profileAvatarPath = previousPath;
        avatarData = previousData;
      }
    } catch (error) { status = `Could not use avatar: ${String(error)}`; }
  }

  async function loadAvatar() {
    if (!settings?.profileAvatarPath) { avatarData = ''; return; }
    avatarData = await invoke<string>('read_profile_avatar', { path: settings.profileAvatarPath }).catch(() => '');
  }

  async function resetAvatar() {
    avatarMenu = null;
    if (!settings) return;
    const previousPath = settings.profileAvatarPath;
    const previousData = avatarData;
    settings.profileAvatarPath = undefined;
    avatarData = '';
    if (!(await save())) {
      settings.profileAvatarPath = previousPath;
      avatarData = previousData;
    }
  }

  async function importSteamAvatar(account: { steamId: string; name: string }) {
    avatarMenu = null;
    if (!settings) return;
    status = `Importing ${account.name || 'Steam'} avatar…`;
    const previousPath = settings.profileAvatarPath;
    const previousData = avatarData;
    try {
      const path = await invoke<string>('import_steam_avatar', { steamId: account.steamId });
      settings.profileAvatarPath = path;
      await loadAvatar();
      if (!(await save())) {
        settings.profileAvatarPath = previousPath;
        avatarData = previousData;
      }
    } catch (error) {
      status = `Steam avatar import failed: ${String(error)}`;
    }
  }

  async function loadDiagnostics() {
    try {
      diagnosticData = await invoke<typeof diagnosticData>('diagnostics');
    } catch (error) {
      status = `Could not load diagnostics: ${String(error)}`;
    }
  }

  async function recoverFailedNotifications(dismiss: boolean) {
    try {
      const count = await invoke<number>(dismiss ? 'dismiss_failed_notifications' : 'retry_failed_notifications');
      status = dismiss
        ? `Dismissed ${count} failed notification${count === 1 ? '' : 's'}`
        : `Retrying ${count} failed notification${count === 1 ? '' : 's'}`;
      await loadDiagnostics();
    } catch (error) {
      status = `Notification recovery failed: ${String(error)}`;
    }
  }

  async function checkUpdates(manual = true) {
    if (manual) status = 'Checking for preview updates…';
    try {
      availableUpdate = await invoke<UpdateInfo | null>('check_for_updates', { manual });
      if (availableUpdate) status = `Achievement Watcher ${availableUpdate.version} is available`;
      else if (manual) status = 'Achievement Watcher is up to date';
    } catch (error) {
      if (manual) status = `Update check failed: ${String(error)}`;
    }
  }

  async function skipUpdate() {
    if (!settings || !availableUpdate) return;
    const skippedUpdate = availableUpdate;
    const previousVersion = settings.skippedUpdateVersion;
    settings.skippedUpdateVersion = availableUpdate.version;
    if (await persistSettings()) {
      availableUpdate = null;
      status = 'This preview version will be skipped';
    } else {
      settings.skippedUpdateVersion = previousVersion;
      availableUpdate = skippedUpdate;
    }
  }

  async function installAvailableUpdate() {
    installingUpdate = true;
    status = 'Downloading and verifying the update installer…';
    try { await invoke('install_update'); }
    catch (error) { installingUpdate = false; status = `Update installation failed: ${String(error)}`; }
  }

  async function saveGameConfig() {
    if (!settings || !gameConfig || !gameConfig.executable) return;
    const previousConfig = settings.gameLaunchConfigs[gameConfig.game.gameId];
    settings.gameLaunchConfigs = { ...settings.gameLaunchConfigs, [gameConfig.game.gameId]: {
      executable: gameConfig.executable, arguments: gameConfig.arguments,
    } };
    if (await save()) {
      closeGameConfig();
    } else {
      const restored = { ...settings.gameLaunchConfigs };
      if (previousConfig) restored[gameConfig.game.gameId] = previousConfig;
      else delete restored[gameConfig.game.gameId];
      settings.gameLaunchConfigs = restored;
    }
  }

  async function launchGame(game: GameSummary) {
    gameMenu = null;
    if (!settings?.gameLaunchConfigs[game.gameId] && game.sourceKind !== 'steam') return configureGame(game);
    try {
      await invoke('launch_game', { gameId: game.gameId });
      status = `Started ${game.name}`;
    } catch (error) {
      status = `Could not start ${game.name}: ${String(error)}`;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    if (confirmation) return;
    if (gameMenu) {
      closeGameMenu(true);
    } else if (avatarMenu) {
      closeAvatarMenu(true);
    } else if (gameConfig) {
      closeGameConfig();
    } else if (view === 'settings') {
      if (savingSettings) return;
      if (settingsDirty()) {
        requestConfirmation(
          'Discard settings changes?',
          'Your unsaved settings will be discarded.',
          'Discard changes',
          cancelSettings,
        );
      } else {
        void cancelSettings();
      }
    } else if (selectedGame) {
      closeGameDetails();
    }
  }

  async function initializeApp() {
    initializing = true;
    startupError = '';
    status = 'Loading saved library…';
    try {
      await invoke('import_legacy').catch((error) => {
        status = `Legacy import skipped: ${String(error)}`;
      });
      settings = await invoke<AppSettings>('load_settings');
      [defaultScreenshotDirectory, defaultClipDirectory] = await Promise.all([
        invoke<string>('default_screenshot_directory'),
        invoke<string>('default_clip_directory'),
      ]);
      applyLanguage();
      await Promise.all([loadDiagnostics(), loadAvatar()]);
      operation = await invoke<OperationSnapshot>('operation_status').catch(() => null);
      await refresh();
      initializing = false;
      status = games.length ? 'Refreshing library…' : 'Searching for achievement data…';
      void (async () => {
        const discoveryError = await detectSources(false, false);
        steamAccounts = await invoke<typeof steamAccounts>('steam_accounts').catch(() => []);
        await useDefaultSteamName();
        const scanSucceeded = await scan(true);
        if (scanSucceeded) {
          status = discoveryError
            ? `Library loaded, but automatic folder detection failed: ${discoveryError}`
            : 'Library is ready';
          void refreshMissingMetadata(status, !discoveryError);
        }
        void checkUpdates(false);
      })();
    } catch (error) {
      startupError = String(error);
      status = 'Startup did not finish';
    } finally {
      initializing = false;
    }
  }

  onMount(() => {
    let disposed = false;
    const cleanup: Array<() => void> = [];
    void initializeApp();
    void consumeOpenGameRequest();
    void appWindow.isMaximized().then((value) => maximized = value).catch(() => undefined);
    void appWindow.onResized(async () => {
      maximized = await appWindow.isMaximized().catch(() => maximized);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else cleanup.push(unlisten);
    }).catch(() => undefined);
    const register = async (event: string, handler: Parameters<typeof listen>[1]) => {
      try {
        const unlisten = await listen(event, handler);
        if (disposed) unlisten();
        else cleanup.push(unlisten);
      } catch (error) {
        liveUpdateErrors = [...new Set([...liveUpdateErrors, event])];
        console.error(`Could not register ${event}:`, error);
      }
    };
    void register('library-changed', () => { void refresh().catch((error) => { status = `Library refresh failed: ${String(error)}`; }); });
    void register('notification-status', (({ payload }: { payload: { transport: string; success: boolean; error?: string } }) => {
        status = notificationStatusMessage(payload, settings?.notificationMode ?? 'overlay_with_native_fallback');
      }) as Parameters<typeof listen>[1]);
    void register('operation-status', (({ payload }: { payload: OperationSnapshot }) => {
        operation = payload;
      }) as Parameters<typeof listen>[1]);
    void register('open-game', (({ payload }: { payload: OpenGameRequest }) => {
        void consumeOpenGameRequest(payload);
      }) as Parameters<typeof listen>[1]);
    return () => {
      disposed = true;
      cleanup.splice(0).forEach((unlisten) => unlisten());
    };
  });
</script>

<svelte:head><title>Achievement Watcher</title></svelte:head>
<svelte:window onclick={() => { closeGameMenu(); closeAvatarMenu(); }} onkeydown={handleWindowKeydown} />

<TitleBar
  activity={titleActivity()}
  settingsActive={view === 'settings'}
  {maximized}
  onMinimize={() => { void runWindowAction('minimize', () => appWindow.minimize()); }}
  onSettings={openSettings}
  onMaximize={() => { void toggleMaximize(); }}
  onClose={closeWindow}
/>

<main>
  {#if view === 'library'}
  {#if selectedGame}
    <section id="achievement" aria-label={`${selectedGame.name} achievements`}>
      {#if selectedGame.icon}<img class="game-background" src={imageUrl(selectedGame.icon)} alt="" />{/if}
      <div class="achievement-page-header">
        <div class="game-heading">
          {#if selectedGame.icon}<img class="detail-game-icon" src={imageUrl(selectedGame.icon)} alt="" />{:else}<span class="detail-game-icon fallback">{selectedGame.name.slice(0, 1).toUpperCase()}</span>{/if}
          <div><h2>{selectedGame.name}</h2><div class="game-source-line"><SourceBadge source={activeSourceKind()} description={activeAchievementSource === 'merged' ? 'Merged from every enabled source' : sourceDescription(activeSourceKind())} />{#if gameSourceChoices.length > 1}<select class="game-source-select" bind:value={activeAchievementSource} onchange={changeAchievementSource} aria-label="Achievement progress source" title="Choose which progress source these achievements use">{#each gameSourceChoices as choice}<option value={choice.sourceId}>{sourceChoiceLabel(choice)}</option>{/each}</select>{:else}<span title={sourceDescription(activeSourceKind())}>{sourceLabel(activeSourceKind())} progress</span>{/if}</div></div>
        </div>
        <div class="game-activity"><div class="achievement-summary"><strong>{detailUnlocked()} / {achievements.length}</strong><span>{achievements.length ? Math.round(detailUnlocked() / achievements.length * 100) : 0}%</span></div>{#if achievements.some((achievement) => achievement.trophyGrade)}<ul class="detail-trophies trophy-totals" aria-label="Unlocked trophies"><li class="platinum" title="Platinum trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('platinum')}</li><li class="gold" title="Gold trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('gold')}</li><li class="silver" title="Silver trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('silver')}</li><li class="bronze" title="Bronze trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('bronze')}</li></ul>{/if}</div>
        <button class="back-button" aria-label="Back to games" title="Back to games" onclick={closeGameDetails}><i class="fas fa-chevron-left"></i></button>
      </div>
      <div class="achievement-tools"><div id="achievement-search"><span><i class="fas fa-search"></i></span><input class:has={achievementQuery.length > 0} type="search" bind:value={achievementQuery} placeholder="Search achievements" aria-label="Search achievements" /></div></div>
      {#if achievementStatus}<p class="detail-status" role="status" aria-live="polite">{achievementStatus}</p>{/if}
      {#if achievementQuery && achievements.length > 0 && visibleAchievementCount() === 0}
        <div class="empty compact">
          <i class="fas fa-search" aria-hidden="true"></i>
          <strong>No matching achievements</strong>
          <span>Try a different name or description.</span>
          <button onclick={() => achievementQuery = ''}>Clear search</button>
        </div>
      {/if}
      {#if achievements.length > 0 && (!achievementQuery || visibleAchievementCount() > 0)}
      {#each [['Unlocked', true, false], ['Locked', false, false], ['Hidden', false, true]] as group}
        {@const achieved = group[1] as boolean}
        {@const hiddenOnly = group[2] as boolean}
        {@const rows = achievementRows(achieved, hiddenOnly)}
        {@const collapsed = achieved ? unlockedCollapsed : hiddenOnly ? hiddenCollapsed : lockedCollapsed}
        {@const groupCount = achievements.filter((achievement) => achievement.achieved === achieved && (achieved || achievement.hidden === hiddenOnly)).length}
        {#if (!hiddenOnly || settings?.showHidden || revealHiddenForGame) && (rows.length || (!achievementQuery && (achieved || (!hiddenOnly && hiddenLockedCount() > 0))))}
          <section class="achievement-group">
            <h3><span><i class={achieved ? 'fas fa-unlock' : hiddenOnly ? 'fas fa-eye-slash' : 'fas fa-lock'}></i> {group[0]} <small>{groupCount}</small></span><span class="achievement-sort" role="group" aria-label={`Sort ${String(group[0]).toLowerCase()} achievements`}><button class:active={achievementSort === 'name'} aria-pressed={achievementSort === 'name'} title="Sort achievements alphabetically" aria-label="Sort achievements alphabetically" onclick={() => achievementSort = 'name'}><i class="fas fa-sort-alpha-down"></i></button>{#if achieved}<button class:active={achievementSort === 'time'} aria-pressed={achievementSort === 'time'} title="Sort by unlock time" aria-label="Sort by unlock time" onclick={() => achievementSort = 'time'}><i class="far fa-clock"></i></button>{/if}{#if !achieved}<button class:active={achievementSort === 'progress'} aria-pressed={achievementSort === 'progress'} title="Sort by progress" aria-label="Sort by progress" onclick={() => achievementSort = 'progress'}><i class="fas fa-percent"></i></button>{/if}<button class:active={achievementSort === 'rarity'} aria-pressed={achievementSort === 'rarity'} title="Sort by global rarity" aria-label="Sort by global rarity" onclick={() => achievementSort = 'rarity'}><i class="fas fa-gem"></i></button></span><button class="collapse-toggle" class:active={!collapsed} aria-expanded={!collapsed} aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${group[0]} achievements`} onclick={() => achieved ? unlockedCollapsed = !unlockedCollapsed : hiddenOnly ? hiddenCollapsed = !hiddenCollapsed : lockedCollapsed = !lockedCollapsed}><i class="fas fa-chevron-right"></i></button></h3>
            {#if !collapsed}<ul>
              {#if !achievementQuery && achieved && rows.length === 0}<li class="achievement-notice"><i class="fas fa-frown-open"></i><strong>{t('noneUnlocked', 'No achievement unlocked yet')}</strong><span>{t('play', 'Start playing!')}</span></li>{/if}
              {#each rows as achievement}
                <li><article data-achievement-id={achievement.achievementId} class:highlight={highlightedAchievement === achievement.achievementId} class:unlocked={achievement.achieved} class:rare={(achievement.globalPercentHundredths ?? 10_001) <= 1000} class="achievement-row">
                  <div class="achievement-icon"><span><i class={achievement.achieved ? 'fas fa-trophy' : 'fas fa-lock'}></i></span>{#if achievement.icon}<img src={imageUrl(achievement.icon)} alt="" onerror={(event) => event.currentTarget.remove()} />{/if}</div>
                  <div class="achievement-content"><h4>{achievement.displayName ?? achievement.achievementId}</h4><p>{achievement.hidden && !achievement.achieved && !settings?.showHidden && !revealHiddenForGame ? t('revealedOnceUnlocked', 'Details for this achievement will be revealed once unlocked') : (achievement.description ?? 'No description available.')}</p>{#if !achievement.achieved && achievement.maxProgress > 0}<div class="achievement-progress" role="progressbar" aria-label={`${achievement.displayName ?? achievement.achievementId} progress`} aria-valuemin="0" aria-valuemax={achievement.maxProgress} aria-valuenow={achievement.currentProgress}><i style={`width:${Math.min(100, achievement.currentProgress / achievement.maxProgress * 100)}%`}></i><span>{achievement.currentProgress} / {achievement.maxProgress}</span></div>{/if}</div>
                  <div class="achievement-state">{#if achievement.originSourceId}<span class="achievement-origin-label" title={sourceDescription(kindForSource(achievement.originSourceId))}><SourceBadge source={kindForSource(achievement.originSourceId)} origin />{sourceLabel(kindForSource(achievement.originSourceId))}</span>{/if}{#if achievement.trophyGrade}<i class={`trophy-grade ${achievement.trophyGrade} fas fa-trophy`} title={`${achievement.trophyGrade} trophy`}></i>{/if}{#if achievement.achieved}<strong>{t('unlocked', 'Unlocked')}</strong>{#if achievement.unlockTime > 0}<time title={new Date(achievement.unlockTime * 1000).toLocaleString()}>{formatUnlockTime(achievement.unlockTime)}</time>{/if}{:else}<span>{t('locked', 'Locked')}</span>{/if}{#if achievement.globalPercentHundredths !== undefined}<small title="Global unlock percentage reported by this achievement source"><i class="fas fa-gem"></i> {achievement.globalPercentHundredths === 0 ? '<0.01' : (achievement.globalPercentHundredths / 100).toFixed(2)}% {t('globalStat', 'of players have this')}</small>{/if}</div>
                </article></li>
              {/each}
            </ul>{#if !achieved && !hiddenOnly && !settings?.showHidden && !revealHiddenForGame && hiddenLockedCount() > 0}<div class="hidden-disclaimer"><span><i class="fas fa-eye-slash"></i> {hiddenLockedCount()} {t('hiddenRemain', 'hidden achievements remaining')}</span><button onclick={() => revealHiddenForGame = true}>{t('settings.common.show', 'Show')} hidden achievements</button></div>{/if}{/if}
          </section>
        {/if}
      {/each}
      {/if}
      <button class="scroll-top" title="Scroll to top" aria-label="Scroll to top" onclick={() => document.getElementById('achievement')?.scrollTo({ top: 0, behavior: 'smooth' })}><i class="fas fa-chevron-up"></i></button>
    </section>
  {:else}
    <section id="home" aria-labelledby="library-title">
      <div id="user-info"><button class="avatar" class:squared={settings?.profileAvatarSquared} title="Choose profile avatar (right-click for options)" aria-label="Choose profile avatar" onclick={chooseAvatar} oncontextmenu={showAvatarMenu}><img src={avatarData || (activeSteamAccount()?.avatarPath ? imageUrl(activeSteamAccount()!.avatarPath!) : defaultAvatar)} alt="" /></button><div class="info"><h1>{settings?.username || activeSteamAccount()?.name || 'Local library'}</h1><ul><li><i class="fas fa-trophy"></i> <strong>{totalUnlocked()}</strong> unlocked</li><li><i class="fas fa-gamepad"></i> <strong>{completedGames()} of {gamesWithAchievements()}</strong> complete</li><li><i class="fas fa-cookie-bite"></i> <strong>{averageCompletion()}%</strong> average</li></ul>{#if trophyTotal('platinum') + trophyTotal('gold') + trophyTotal('silver') + trophyTotal('bronze') > 0}<ul class="trophy-totals" aria-label="PlayStation trophies"><li class="platinum" title="Platinum trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('platinum')}</strong></li><li class="gold" title="Gold trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('gold')}</strong></li><li class="silver" title="Silver trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('silver')}</strong></li><li class="bronze" title="Bronze trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('bronze')}</strong></li></ul>{/if}</div></div>
      <div class="library-tools"><div id="search-bar"><span><i class="fas fa-search"></i></span><input class:has={query.length > 0} type="search" bind:value={query} placeholder="Search games" aria-label="Search games" /></div><select bind:value={libraryFilter} aria-label="Filter games"><option value="all">All games</option><option value="tracked">Tracked games</option>{#if settings?.showCachedGames}<option value="cached">Cached games only</option>{/if}</select><button class="refresh" title="Refresh library" aria-label="Refresh library" onclick={() => scan(false)} disabled={scanning}><i class="fas fa-sync-alt" class:fa-spin={scanning}></i></button><div id="sort-box" role="group" aria-label="Sort games"><button class:active={librarySort === 'name'} aria-pressed={librarySort === 'name'} title="Sort alphabetically" aria-label="Sort alphabetically" onclick={() => librarySort = 'name'}><i class="fas fa-sort-alpha-down"></i></button><button class:active={librarySort === 'progress'} aria-pressed={librarySort === 'progress'} title="Sort by completion" aria-label="Sort by completion" onclick={() => librarySort = 'progress'}><i class="fas fa-sort-numeric-down"></i><i class="fas fa-percent"></i></button><button class:active={librarySort === 'recent'} aria-pressed={librarySort === 'recent'} title="Sort by most recent unlock" aria-label="Sort by most recent unlock" onclick={() => librarySort = 'recent'}><i class="fas fa-sort-numeric-down"></i><i class="far fa-clock"></i></button></div></div>
      <div id="game-list" class:view-portrait={settings?.thumbnailPortrait}>
      {#if initializing}<div class="empty"><i class="fas fa-circle-notch fa-spin" aria-hidden="true"></i><strong>Loading library</strong><span>Reading saved games and achievement sources…</span></div>{:else if startupError}<div class="empty error" role="alert"><i class="fas fa-exclamation-triangle" aria-hidden="true"></i><strong>Could not load the library</strong><span>{startupError}</span><div class="empty-actions"><button onclick={initializeApp}>Retry</button><button onclick={() => invoke('open_data_location', { location: 'data', path: null }).catch((error) => status = String(error))}>Open data folder</button></div></div>{:else if games.length === 0}<div class="empty"><i class="fas fa-gamepad" aria-hidden="true"></i><strong>No games found</strong><span>Achievement folders are detected automatically. Check Settings if a folder is missing.</span><button onclick={openSettings}>Open settings</button></div>{:else if visibleGames().length === 0}<div class="empty"><i class="fas fa-search" aria-hidden="true"></i><strong>No matching games</strong><span>Your library is intact. Clear the search or filter to see it.</span><button onclick={clearLibraryFilters}>Clear filters</button></div>{:else}<ul>{#each visibleGames() as game}<li><article class="game-box" oncontextmenu={(event) => showGameMenu(event, game)}><button class="game-open" onclick={() => openGame(game)} aria-label={`${game.name}, ${game.unlocked} of ${game.total} achievements unlocked, ${game.sourceId === 'merged' ? 'combined sources' : sourceLabel(game.sourceKind)}`} title={`${game.name} — ${game.sourceId === 'merged' ? 'Merged from every enabled source' : sourceDescription(game.sourceKind)}`}><span class="game-header"><span>{game.name.slice(0, 1).toUpperCase()}</span>{#if gameArtwork(game)}<img src={gameArtwork(game)} alt="" onerror={(event) => gameArtworkFailed(event, game)} />{/if}</span><span class="game-info"><span><strong>{game.name}</strong><SourceBadge source={game.sourceKind} description={game.sourceId === 'merged' ? 'Merged from every enabled source' : sourceDescription(game.sourceKind)} /></span><span class="game-progress" role="progressbar" aria-label={`${game.name} completion`} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(completionPercent(game))} data-percent={Math.round(completionPercent(game))}><i style={`width:${completionPercent(game)}%`}></i></span></span></button><button class="game-menu-button" title={`More actions for ${game.name}`} aria-label={`More actions for ${game.name}`} onclick={(event) => showGameMenu(event, game)}><i class="fas fa-ellipsis-v"></i></button></article></li>{/each}</ul>{/if}
      </div>
    </section>
  {/if}
  {:else}
  {#if settings}
    <section id="settings" aria-labelledby="settings-title">
      <div class="settings-box">
      <div class="settings-header"><i class="fas fa-cog"></i><h2 id="settings-title">{t('settings.title', 'Settings')}</h2></div>
      <div class="settings-container">
      <nav class="settings-nav" aria-label="Settings categories">
        {#each [['general','fas fa-tools','General'],['notification','fas fa-bell','Notifications'],['souvenir','fas fa-camera','Captures'],['folder','far fa-folder','Folders'],['source','fas fa-file-import','Accounts & sources'],['advanced','fas fa-plug','Integrations'],['debug','fas fa-bug','Diagnostics']] as tab}
          <button class:active={settingsTab === tab[0]} aria-current={settingsTab === tab[0] ? 'page' : undefined} onclick={() => { settingsTab = tab[0] as typeof settingsTab; if (tab[0] === 'debug') void loadDiagnostics(); }}><i class={tab[1]} aria-hidden="true"></i>{tab[2]}</button>
        {/each}
      </nav>
      <div class="settings-content">
      {#if settingsError}<div class="settings-error" role="alert"><i class="fas fa-exclamation-triangle" aria-hidden="true"></i><span>{settingsError}</span></div>{/if}
      {#if settingsTab === 'general'}
      <div class="settings-group">
        <h3>Interface and library</h3>
        <div class="field"><label for="display-name">Display name</label><input id="display-name" bind:value={settings.username} onchange={() => { if (settings) settings.usernameCustomized = true; void save(); }} placeholder="Steam name" />{#if activeSteamAccount()?.name && settings.username !== activeSteamAccount()?.name}<button type="button" title={`Restore ${activeSteamAccount()!.name}`} onclick={restoreSteamName}>Use Steam name</button>{/if}</div>
        <div class="field"><span>Profile avatar</span><button onclick={chooseAvatar}>{settings.profileAvatarPath ? 'Change' : 'Choose'}</button>{#if settings.profileAvatarPath}<button onclick={() => { if (settings) { settings.profileAvatarPath = undefined; avatarData = ''; void save(); } }}>Remove</button>{/if}</div>
        <label class="field"><span>Game thumbnails</span><select bind:value={settings.thumbnailPortrait} onchange={save}><option value={false}>Landscape</option><option value={true}>Portrait</option></select></label>
        <label class="check"><input type="checkbox" bind:checked={settings.showCachedGames} onchange={save} /> Show games that have cached information but no tracked progress</label>
        <label class="check"><input type="checkbox" bind:checked={settings.hideZero} onchange={save} /> Hide games with no unlocked achievements</label>
        <label class="check"><input type="checkbox" bind:checked={settings.showHidden} onchange={save} /> Reveal hidden achievement names and descriptions</label>
        <label class="check"><input type="checkbox" bind:checked={settings.mergeDuplicate} onchange={save} /> Merge the same game when it is found in multiple sources</label>
        <label class="check"><input type="checkbox" bind:checked={settings.showPlayButton} onchange={save} /> Show Play actions for configured games</label>
        {#if settings.mergeDuplicate}<label class="check nested"><input type="checkbox" bind:checked={settings.timeMergeRecentFirst} onchange={save} /> Keep the most recent unlock timestamp when sources disagree</label>{/if}
      </div>
      <div class="settings-group">
        <h3>Background behavior</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.runAtLogin} onchange={save} /> Run Achievement Watcher when I sign in</label>
        {#if settings.runAtLogin}<label class="check nested"><input type="checkbox" bind:checked={settings.startMinimized} onchange={save} /> Start hidden in the system tray</label>{/if}
        <label class="check"><input type="checkbox" bind:checked={settings.closeToTray} onchange={save} /> Keep watching when the main window is closed</label>
        <p class="settings-help">Use Quit from the tray menu to stop achievement monitoring completely.</p>
      </div>
      <div class="settings-group">
        <h3>Hidden games</h3>
        {#if blacklistedGames.length > 0}<div class="blacklisted-games">{#each blacklistedGames as game}<div><span><strong>{game.name}</strong>{#if game.name !== game.gameId}<small>{game.gameId}</small>{/if}</span><button onclick={() => restoreBlacklistedGame(game.gameId)}>Restore</button></div>{/each}</div>{:else}<p class="muted">No games are hidden.</p>{/if}
        <div class="field"><span>{settings.blacklistedGameIds.length} hidden game{settings.blacklistedGameIds.length === 1 ? '' : 's'}</span><button disabled={settings.blacklistedGameIds.length === 0} onclick={() => requestConfirmation('Restore every hidden game?', 'Every hidden game will become eligible to appear in the library again after you save.', 'Restore all', () => { if (settings) { settings.blacklistedGameIds = []; blacklistedGames = []; } })}>Restore all</button></div>
      </div>
      <div class="settings-group">
        <h3>Updates</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.checkForUpdates} onchange={save} /> Check for new preview releases automatically</label>
        <div class="field"><span>Application updates</span><button onclick={() => checkUpdates(true)}>Check now</button></div>
      </div>
      {:else if settingsTab === 'notification'}
      <div class="settings-group">
        <h3>Achievement notifications</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.notificationEnabled} onchange={save} /> Show achievement notifications</label>
        {#if settings.notificationEnabled}
        <label class="check nested"><input type="checkbox" bind:checked={settings.notifyOnProgress} onchange={save} /> Notify when achievement progress changes</label>
        <label class="check nested"><input type="checkbox" bind:checked={settings.notifyOnPlaytime} onchange={save} /> Notify when playtime tracking starts and stops</label>
        <label class="check nested"><input type="checkbox" bind:checked={settings.notificationShowDescription} onchange={save} /> Show achievement descriptions</label>
        <label class="field"><span>Desktop delivery</span><select bind:value={settings.notificationMode} onchange={save}><option value="overlay_with_native_fallback">Custom popup with Windows fallback</option><option value="overlay_only">Custom popup only</option><option value="native_only">Windows notification only</option></select></label>
        {#if settings.notificationMode !== 'native_only'}
          <label class="field"><span>Popup style</span><select bind:value={settings.notificationPreset} onchange={save}><option value="steam">Steam</option><option value="default">Achievement Watcher Default</option><option value="smooth_pop">SmoothPop</option><option value="ps4">PlayStation 4</option><option value="ps5">PlayStation 5</option><option value="ps5_enhanced">PlayStation 5 Enhanced</option><option value="xbox_one">Xbox One</option><option value="xbox_360">Xbox 360</option><option value="raposo">Raposo</option><option value="xqjan">xqjan</option></select></label>
          <label class="field"><span>Sound</span><select bind:value={settings.notificationSound} onchange={save}><option value="steam_deck">Steam Deck</option><option value="windows">Windows 10</option><option value="windows_11">Windows 11</option><option value="playstation">PlayStation</option><option value="playstation_5">PlayStation 5</option><option value="playstation_platinum">PlayStation 5 Platinum</option><option value="gog">GOG Galaxy</option><option value="android">Android Popcorn</option>{#if settings.notificationCustomSoundPath}<option value="custom">Custom file</option>{/if}<option value="none">None</option></select><button type="button" onclick={chooseNotificationSound}>Browse…</button></label>
          {#if settings.notificationSound === 'custom'}<div class="folder-setting nested"><span>Custom sound</span><code title={settings.notificationCustomSoundPath}>{settings.notificationCustomSoundPath}</code><button onclick={chooseNotificationSound}>Change</button><button onclick={() => { if (settings) { settings.notificationCustomSoundPath = undefined; settings.notificationSound = 'steam_deck'; void save(); } }}>Reset</button></div>{/if}
          <label class="check nested"><input type="checkbox" bind:checked={settings.rumbleEnabled} onchange={save} /> Vibrate connected Xbox-compatible controllers on unlock</label>
          {#if settings.rumbleEnabled}<label class="field nested"><span>Rumble strength</span><input type="range" min="1" max="100" bind:value={settings.rumbleStrengthPercent} onchange={save} /><output>{settings.rumbleStrengthPercent}%</output></label><label class="field nested"><span>Rumble duration</span><select bind:value={settings.rumbleDurationMs} onchange={save}><option value={250}>Short</option><option value={450}>Normal</option><option value={800}>Long</option></select></label>{/if}
          <label class="field"><span>Animation duration</span><input type="range" min="10" max="500" step="10" bind:value={settings.notificationDurationPercent} onchange={save} /><output>{settings.notificationDurationPercent}%</output></label>
          <label class="field"><span>Popup scale</span><input type="range" min="50" max="150" step="5" bind:value={settings.notificationScalePercent} onchange={save} /><output>{settings.notificationScalePercent}%</output></label>
          <label class="field"><span>Screen position</span><select bind:value={settings.notificationPosition} onchange={save}><option value="bottom_right">Bottom right</option><option value="bottom_center">Center bottom</option><option value="bottom_left">Bottom left</option><option value="top_right">Top right</option><option value="top_center">Center top</option><option value="top_left">Top left</option></select></label>
        {/if}
        <div class="field"><span>Preview</span><button onclick={testNotification}>Test notification</button></div>
        {/if}
      </div>
      <div class="settings-group">
        <h3>Fullscreen delivery</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.gameBarEnabled} onchange={save} /> Use Xbox Game Bar companion when available</label>
        {#if settings.gameBarEnabled}<label class="check nested"><input type="checkbox" bind:checked={settings.gameBarFullscreenOnly} onchange={save} /> Use the companion only while a fullscreen app is active</label><div class="token-row"><span>Pairing token</span><code>{settings.gameBarToken}</code><button onclick={() => invoke('test_game_bar', { settings }).then(() => status = 'Game Bar acknowledged the test').catch((error) => status = `Game Bar test failed: ${String(error)}`)}>Test</button></div>{/if}
      </div>
      <div class="settings-group">
        <h3>In-game achievement overlay</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.achievementOverlayEnabled} onchange={save} /> Show the current game's achievement list over a running game</label>
        {#if settings.achievementOverlayEnabled}<label class="field"><span>Toggle shortcut</span><input bind:value={settings.achievementOverlayHotkey} onchange={save} aria-label="Achievement overlay shortcut" /></label><label class="field"><span>Overlay scale</span><input type="range" min="50" max="200" step="5" bind:value={settings.achievementOverlayScalePercent} onchange={save} /><output>{settings.achievementOverlayScalePercent}%</output></label><div class="field"><span>Current game</span><button onclick={() => invoke('toggle_achievement_overlay').then(() => status = 'Achievement overlay toggled').catch((error) => status = `Overlay unavailable: ${String(error)}`)}>Toggle overlay</button></div>{/if}
        <p class="settings-help">The overlay is created only while visible and closes completely when toggled off. Exclusive-fullscreen games may require the Xbox Game Bar companion.</p>
      </div>
      <div class="settings-group">
        <h3>Unlock filtering</h3>
        <label class="field"><span>Maximum event age</span><input type="number" min="0" max="3600" bind:value={settings.notificationMaxAgeSeconds} onchange={save} /><small>seconds</small></label>
        <label class="check"><input type="checkbox" bind:checked={settings.notificationRequireRunningGame} onchange={save} /> Require the configured game executable or a fullscreen app to be running</label>
        <p class="settings-help">Games without a configured executable continue to notify. Set the maximum age to 0 to accept only unlocks stamped at the current second.</p>
      </div>
      {:else if settingsTab === 'souvenir'}
      <div class="settings-group">
        <h3>Screenshot</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.screenshotEnabled} onchange={save} /> Save a screenshot when an achievement unlocks</label>
        {#if settings.screenshotEnabled}<p class="settings-help">Screenshots are organized into a folder for each game. Repeated unlocks are saved as additional timestamped copies.</p><div class="folder-setting"><span>Save location</span><code>{settings.screenshotDirectory ?? defaultScreenshotDirectory}</code><button onclick={() => invoke('open_data_location', { location: 'screenshots', path: settings?.screenshotDirectory ?? defaultScreenshotDirectory }).catch((error) => status = String(error))}>Open</button><button onclick={chooseScreenshotFolder}>Choose</button>{#if settings.screenshotDirectory}<button onclick={() => { if (settings) { settings.screenshotDirectory = undefined; void save(); } }}>Reset</button>{/if}</div>{/if}
      </div>
      <div class="settings-group">
        <h3>Achievement clips</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.obsReplayEnabled} onchange={save} /> Record a clip when an achievement unlocks <small>(requires OBS Studio)</small></label>
        {#if settings.obsReplayEnabled}
          <p class="settings-help">Achievement clips use OBS Studio's replay buffer and WebSocket server. The original replay remains in OBS's recording folder and a copy is organized here by game.</p>
          <div class="folder-setting"><span>Save location</span><code>{settings.clipDirectory ?? defaultClipDirectory}</code><button onclick={() => invoke('open_data_location', { location: 'clips', path: settings?.clipDirectory ?? defaultClipDirectory }).catch((error) => status = String(error))}>Open</button><button onclick={chooseClipFolder}>Choose</button>{#if settings.clipDirectory}<button onclick={() => { if (settings) { settings.clipDirectory = undefined; void save(); } }}>Reset</button>{/if}</div>
          <label class="field"><span>Host</span><input bind:value={settings.obsHost} onchange={save} /></label>
          <label class="field"><span>Port</span><input type="number" min="1" max="65535" bind:value={settings.obsPort} onchange={save} /></label>
          <label class="field"><span>Password</span><input type="password" bind:value={settings.obsPassword} onchange={save} autocomplete="off" /></label>
          <label class="check nested"><input type="checkbox" bind:checked={settings.obsStartReplayBuffer} onchange={save} /> Start recording automatically when needed</label>
          <div class="field"><span>Clip recording</span><button onclick={() => invoke('test_obs', { settings }).then(() => status = 'Test clip saved').catch((error) => status = `Clip test failed: ${String(error)}`)}>Run test</button></div>
        {/if}
      </div>
      {:else if settingsTab === 'folder'}
      <div class="settings-group">
        <h3>Achievement folders</h3>
        <p class="settings-help">Achievement Watcher finds common locations automatically. Add a folder only when a source is stored somewhere unusual.</p>
        <div class="sources">
          {#each settings.sourceLocations as source}
            <div class="source"><label><input type="checkbox" bind:checked={source.enabled} onchange={save} />{sourceLabel(source.kind)}</label><label title="Allow unlock notifications and souvenir actions from this folder"><input type="checkbox" bind:checked={source.notify} onchange={save} disabled={!source.enabled} />Notify</label><code title={source.path}>{source.path}</code><button aria-label={`Remove ${source.path}`} onclick={() => removeSource(source.id)}>Remove</button></div>
          {:else}<p class="muted">No live achievement folders were detected. Cached games can still be browsed, but unlocks are not monitored.</p>{/each}
        </div>
        <div class="source-actions"><button title="Search every local drive for supported achievement folders" onclick={() => detectSources(true, false)}>Search all drives</button><button onclick={() => addSource('steam_emulator')}>Add Steam emulator folder</button><button onclick={() => addSource('rpcs3')}>Add RPCS3 folder</button><button onclick={() => addSource('epic')}>Add Epic emulator folder</button><button onclick={() => addSource('gog')}>Add GOG emulator folder</button></div>
      </div>
      {:else if settingsTab === 'source'}
      <div class="settings-group">
        <h3>Official Steam</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.steamEnabled} onchange={save} /> Import achievements from the signed-in Steam client</label>
        {#if settings.steamEnabled}<label class="field"><span>Games to display</span><select bind:value={settings.steamLibraryMode} onchange={save}><option value="played">Games with local Steam stats</option><option value="installed">Installed</option><option value="owned">Owned (public profile or API key)</option></select></label><label class="check"><input type="checkbox" bind:checked={settings.steamPublicFallback} onchange={save} /> Use the public Steam profile when client data is unavailable</label>{#if steamAccounts.length}<label class="field"><span>Steam account</span><select bind:value={settings.steamAccountId} onchange={save}><option value={undefined}>Most recently used account</option>{#each steamAccounts as account}<option value={account.accountId}>{account.name || account.steamId}{account.mostRecent ? ' (recent)' : ''}</option>{/each}</select></label>{:else}<label class="field"><span>Steam account</span><input bind:value={settings.steamAccountId} onchange={save} placeholder="Detected automatically" /></label>{/if}{/if}
      </div>
      {#if settings.steamEnabled}<div class="settings-group">
        <h3>Steam Web API fallback</h3>
        <p class="settings-help">Optional fallback for profiles that cannot be read through the signed-in Steam client. Leave blank for local-only operation.</p>
        <label class="field"><span>Web API key</span><input type="password" bind:value={settings.steamApiKey} onchange={save} autocomplete="off" placeholder="Optional" /></label>
      </div>{/if}
      <div class="settings-group">
        <h3>Official GOG Galaxy</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.gogGalaxyEnabled} onchange={save} /> Import achievements from GOG Galaxy's local database</label>
        <p class="settings-help">Galaxy accounts and tokens stay inside GOG Galaxy. Achievement Watcher reads only the local gameplay database.</p>
      </div>
      <div class="settings-group">
        <h3>Emulators and compatibility sources</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.steamEmulatorEnabled} onchange={save} /> Steam emulator saves</label>
        <label class="check"><input type="checkbox" bind:checked={settings.greenLumaEnabled} onchange={save} /> GreenLuma registry saves</label>
        <label class="check"><input type="checkbox" bind:checked={settings.rpcs3Enabled} onchange={save} /> RPCS3 trophies</label>
        <label class="check"><input type="checkbox" bind:checked={settings.epicEnabled} onchange={save} /> Nemirtingas Epic emulator</label>
        <label class="check"><input type="checkbox" bind:checked={settings.gogEnabled} onchange={save} /> Nemirtingas Galaxy emulator</label>
        <label class="check"><input type="checkbox" bind:checked={settings.lumaPlayEnabled} onchange={save} /> LumaPlay registry achievements</label>
        <label class="check"><input type="checkbox" bind:checked={settings.watchdogCacheEnabled} onchange={save} /> Original Achievement Watcher cache</label>
      </div>
      {:else if settingsTab === 'advanced'}
      <div class="settings-group">
        <h3>Unlock action</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.customActionEnabled} onchange={save} /> Run a program after an achievement unlocks</label>
        {#if settings.customActionEnabled}<div class="folder-setting"><span>Program</span><code>{settings.customActionExecutable || 'Not selected'}</code><button onclick={chooseCustomAction}>Choose</button></div><div class="folder-setting"><span>Working folder</span><code>{settings.customActionWorkingDirectory || 'Program folder'}</code><button onclick={chooseCustomActionDirectory}>Choose</button>{#if settings.customActionWorkingDirectory}<button onclick={() => { if (settings) { settings.customActionWorkingDirectory = undefined; void save(); } }}>Reset</button>{/if}</div><label class="field"><span>Arguments</span><input bind:value={settings.customActionArguments} onchange={save} placeholder={'{game_id} {achievement_id}'} /></label><label class="check"><input type="checkbox" bind:checked={settings.customActionHideWindow} onchange={save} /> Hide the program window</label><p class="settings-help">Available placeholders: {`{game_id}`}, {`{achievement_id}`}, {`{name}`}, and {`{source}`}.</p>{/if}
      </div>
      <div class="settings-group">
        <h3>WebSocket transport</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.websocketEnabled} onchange={save} /> Broadcast achievement events to local integrations</label>
        {#if settings.websocketEnabled}<label class="field"><span>Listen address</span><input bind:value={settings.websocketHost} onchange={save} /></label><label class="field"><span>Port</span><input type="number" min="1" max="65535" bind:value={settings.websocketPort} onchange={save} /></label><p class="settings-help">Compatible clients can connect to ws://{settings.websocketHost}:{settings.websocketPort}.</p>{/if}
      </div>
      <div class="settings-group">
        <h3>Growl (GNTP)</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.gntpEnabled} onchange={save} /> Forward unlocks to a local Growl-compatible receiver</label>
        {#if settings.gntpEnabled}<label class="field"><span>Host</span><input bind:value={settings.gntpHost} onchange={save} /></label><label class="field"><span>Port</span><input type="number" min="1" max="65535" bind:value={settings.gntpPort} onchange={save} /></label><div class="field"><span>Connection</span><button onclick={() => invoke('test_gntp', { settings }).then(() => status = 'GNTP test sent').catch((error) => status = `GNTP test failed: ${String(error)}`)}>Run test</button></div>{/if}
      </div>
      {:else}
      <div class="settings-group">
        <h3>Diagnostics</h3>
        <div class="field"><span>Achievement notification</span><button onclick={testNotification}>Run test</button></div>
        <div class="field"><span>Progress notification</span><button onclick={() => testNotificationKind('test_progress_notification', 'Progress')}>Run test</button></div>
        <div class="field"><span>Playtime notification</span><button onclick={() => testNotificationKind('test_playtime_notification', 'Playtime')}>Run test</button></div>
        <div class="field"><span>Windows notification controls</span><button onclick={() => invoke('open_windows_settings', { page: 'focus_assist' }).catch((error) => status = String(error))}>Focus Assist</button><button onclick={() => invoke('open_windows_settings', { page: 'notifications' }).catch((error) => status = String(error))}>Notifications &amp; actions</button></div>
        <p class="settings-help">Fullscreen Focus Assist rules can suppress native Windows notifications. Custom desktop popups and the optional Game Bar companion use separate delivery paths.</p>
        <div class="field"><span>Scan all sources</span><button onclick={() => scan(false)} disabled={scanning}>Run scan</button></div>
        {#if settings.gameBarEnabled}<div class="field"><span>Xbox Game Bar</span><button onclick={() => invoke('test_game_bar', { settings }).then(() => status = 'Game Bar acknowledged the test').catch((error) => status = `Game Bar test failed: ${String(error)}`)}>Run test</button></div>{/if}
        {#if settings.obsReplayEnabled}<div class="field"><span>Achievement clips</span><button onclick={() => invoke('test_obs', { settings }).then(() => status = 'Test clip saved').catch((error) => status = `Clip test failed: ${String(error)}`)}>Run test</button></div>{/if}
        <div class="field"><span>Runtime status</span><button onclick={loadDiagnostics}>Refresh</button></div>
        {#if diagnosticData}<div class="diagnostic-grid"><span>Version</span><strong>{diagnosticData.appVersion}</strong><span>Games</span><strong>{diagnosticData.gameCount}</strong><span>Achievement records</span><strong>{diagnosticData.observationCount}</strong><span>Enabled folders</span><strong>{diagnosticData.enabledSourceCount}</strong><span>Missing folders</span><strong class:warning={diagnosticData.missingSourceCount > 0}>{diagnosticData.missingSourceCount}</strong><span>Pending notifications</span><strong>{diagnosticData.pendingNotifications}</strong><span>Failed notifications</span><strong class:warning={diagnosticData.failedNotifications > 0}>{diagnosticData.failedNotifications}</strong></div>{#if diagnosticData.watchers.length}<div class="diagnostic multi"><span>Watcher health</span><div class="diagnostic-values">{#each diagnosticData.watchers as watcher}<code class:warning={Boolean(watcher.lastError)}>{watcher.name}: {watcher.enabled ? (watcher.lastError ? watcher.lastError : 'running') : 'not needed'} · checked {new Date(watcher.lastHeartbeatAt * 1000).toLocaleTimeString()}</code>{/each}</div></div>{/if}{#if diagnosticData.failedNotifications > 0}<div class="field"><span>Failed notification queue</span><button onclick={() => recoverFailedNotifications(false)}>Retry now</button><button onclick={(event) => requestConfirmation('Dismiss failed notifications?', 'The queued events will not be retried again.', 'Dismiss events', () => recoverFailedNotifications(true))}>Dismiss</button></div>{/if}{#if diagnosticData.recentErrors.length}<div class="diagnostic multi"><span>Recent delivery errors</span><div class="diagnostic-values">{#each diagnosticData.recentErrors as message}<code>{message}</code>{/each}</div></div>{/if}<div class="diagnostic"><span>Notification log</span><code>{diagnosticData.notificationLog}</code><button onclick={() => invoke('open_data_location', { location: 'notification_log', path: null }).catch((error) => status = String(error))}>Open</button></div>{/if}
      </div>
      {/if}
      </div></div>
      <div class="settings-footer"><div class="settings-notice"><span>Preview v{diagnosticData?.appVersion ?? '…'} ·</span><button onclick={() => invoke('open_project_page', { project: 'fork' })}>darktakayanagi/achievement-watcher</button><span>· Original v1.6.8 ·</span><button onclick={() => invoke('open_project_page', { project: 'original' })}>xan105/achievement-watcher</button></div><div><button disabled={savingSettings} onclick={cancelSettings}>{t('settings.common.cancel', 'Cancel')}</button><button class="primary" onclick={acceptSettings} disabled={savingSettings || !settingsDirty()}>{savingSettings ? 'Saving…' : t('settings.common.save', 'Save')}</button></div></div>
      </div>
    </section>
  {/if}
  {/if}
</main>
{#if availableUpdate}
  <aside class="update-banner" aria-live="polite"><i class="fas fa-download"></i><div><strong>Achievement Watcher {availableUpdate.version} is available</strong><span>{availableUpdate.installerName}</span></div><button onclick={() => invoke('open_release_page', { url: availableUpdate!.releaseUrl })}>Release notes</button><button onclick={skipUpdate}>Skip</button><button class="primary" disabled={installingUpdate} onclick={installAvailableUpdate}>{installingUpdate ? 'Downloading…' : 'Install update'}</button></aside>
{/if}
{#if avatarMenu && settings}
  <div bind:this={avatarMenuElement} class="context-menu" style={`left:${avatarMenu.x}px;top:${avatarMenu.y}px`} role="menu" tabindex="-1" onkeydown={(event) => handleMenuKeydown(event, avatarMenuElement, () => closeAvatarMenu(true))} oncontextmenu={(event) => event.preventDefault()}>
    <div class="context-title">Profile avatar</div>
    <button role="menuitemcheckbox" aria-checked={settings.profileAvatarSquared} onclick={() => { if (settings) { settings.profileAvatarSquared = !settings.profileAvatarSquared; avatarMenu = null; void save(); } }}><i class={settings.profileAvatarSquared ? 'fas fa-check-square' : 'far fa-square'}></i> Squared</button>
    <button role="menuitem" onclick={() => { avatarMenu = null; void chooseAvatar(); }}><i class="fas fa-folder-open"></i> Browse…</button>
    <button role="menuitem" onclick={resetAvatar}><i class="fas fa-redo-alt"></i> Reset to default avatar</button>
    {#each steamAccounts as account}<button role="menuitem" onclick={() => importSteamAvatar(account)}><i class="fab fa-steam"></i> Import {account.name || account.steamId}'s Steam avatar</button>{/each}
  </div>
{/if}
{#if gameMenu}
  <div bind:this={gameMenuElement} class="context-menu" style={`left:${gameMenu.x}px;top:${gameMenu.y}px`} role="menu" tabindex="-1" onkeydown={(event) => handleMenuKeydown(event, gameMenuElement, () => closeGameMenu(true))} oncontextmenu={(event) => event.preventDefault()}>
    <div class="context-title">{gameMenu.game.name}</div>
    {#if settings?.showPlayButton}<button role="menuitem" onclick={() => launchGame(gameMenu!.game)}>Play</button>{/if}
    {#if settings?.showPlayButton}<button role="menuitem" onclick={() => configureGame(gameMenu!.game)}>Configure executable</button>{/if}
    <button role="menuitem" onclick={() => refreshGameMetadata(gameMenu!.game)}>Refresh game information</button>
    <button role="menuitem" onclick={() => { const game = gameMenu!.game; closeGameMenu(); requestConfirmation('Clear cached information?', `${game.name} will keep its local achievement progress, but downloaded names and artwork may need to be fetched again.`, 'Clear cache', () => clearGameMetadata(game)); }}>Clear cached information</button>
    {#if gameMenu.sources === undefined}<div class="context-note">Checking achievement sources…</div>{:else if gameMenu.sourceError}<div class="context-note error" title={gameMenu.sourceError}>Could not check achievement sources</div>{:else if gameMenu.sources.length === 0}<div class="context-note">No local achievement source</div>{:else}{#each gameMenu.sources as source}<button role="menuitem" title={source.sourcePath ? `Open source under ${source.sourcePath}` : sourceDescription(source.sourceKind)} onclick={() => { const game = gameMenu!.game; closeGameMenu(); invoke('open_achievement_source', { sourceId: source.sourceId, gameId: game.gameId }).catch((error) => status = `Could not open achievement source: ${String(error)}`); }}>Open {sourceLabel(source.sourceKind)} source</button>{/each}{/if}
    {#if hasSteamAppId(gameMenu.game)}
      <div class="context-separator"></div>
      <button role="menuitem" onclick={() => openGameWebsite(gameMenu!.game, 'steam')}>Steam store</button>
      <button role="menuitem" onclick={() => openGameWebsite(gameMenu!.game, 'steamdb')}>SteamDB</button>
      <button role="menuitem" onclick={() => openGameWebsite(gameMenu!.game, 'pcgamingwiki')}>PCGamingWiki</button>
    {/if}
    <div class="context-separator"></div>
    <button role="menuitem" class="danger" onclick={() => { const game = gameMenu!.game; closeGameMenu(); requestConfirmation('Hide this game?', `${game.name} will be added to the blacklist and removed from the library.`, 'Hide game', () => blacklistGame(game)); }}>Hide game</button>
  </div>
{/if}
{#if confirmation}
  <ConfirmDialog title={confirmation.title} message={confirmation.message} confirmLabel={confirmation.confirmLabel} busy={confirmationBusy} onConfirm={runConfirmedAction} onCancel={cancelConfirmation} />
{/if}
{#if gameConfig}
  <GameConfigDialog gameName={gameConfig.game.name} bind:executable={gameConfig.executable} bind:launchArguments={gameConfig.arguments} onBrowse={chooseGameExecutable} onSave={saveGameConfig} onCancel={closeGameConfig} />
{/if}
<StatusBar busy={initializing || scanning || savingSettings || installingUpdate || Boolean(operation?.kind)} message={displayedStatus()} />
