<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { onMount, tick } from 'svelte';
  import { completionPercent, sourceDescription, sourceLabel } from './library';
  import steamIcon from '../../app/Source/steam.svg';
  import playstationIcon from '../../app/Source/playstation.svg';
  import epicIcon from '../../app/Source/epic.svg';
  import gogIcon from '../../app/Source/gog.svg';
  import defaultAvatar from '../../app/resources/img/avatar.png';
  import type { AchievementObservation, AppSettings, GameSummary, SourceKind, UpdateInfo } from './types';

  let games: GameSummary[] = [];
  let settings: AppSettings | null = null;
  let settingsSnapshot: AppSettings | null = null;
  let status = 'Preparing local library…';
  let scanning = false;
  let view: 'library' | 'settings' = 'library';
  let selectedGame: GameSummary | null = null;
  let achievements: AchievementObservation[] = [];
  let achievementStatus = '';
  let query = '';
  let libraryFilter: 'all' | 'tracked' | 'cached' = 'all';
  let librarySort: 'name' | 'progress' | 'recent' = 'name';
  let settingsTab: 'general' | 'notification' | 'souvenir' | 'folder' | 'source' | 'advanced' | 'debug' = 'general';
  let gameMenu: { game: GameSummary; x: number; y: number } | null = null;
  let avatarMenu: { x: number; y: number } | null = null;
  let achievementSort: 'name' | 'time' | 'progress' | 'rarity' = 'name';
  let achievementQuery = '';
  let unlockedCollapsed = false;
  let lockedCollapsed = false;
  let steamAccounts: Array<{ accountId: string; steamId: string; name: string; mostRecent: boolean }> = [];
  let gameConfig: { game: GameSummary; executable: string; arguments: string } | null = null;
  let highlightedAchievement = '';
  let revealHiddenForGame = false;
  let avatarData = '';
  let gameSourceChoices: Array<{ sourceId: string; sourceKind?: SourceKind }> = [];
  let activeAchievementSource = '';
  let diagnosticData: { appVersion: string; observationCount: number; gameCount: number; enabledSourceCount: number; missingSourceCount: number; pendingNotifications: number; failedNotifications: number; recentErrors: string[]; notificationLog: string } | null = null;
  let availableUpdate: UpdateInfo | null = null;
  let installingUpdate = false;
  const appWindow = getCurrentWindow();
  const localeModules = import.meta.glob('../../app/locale/lang/*.json', { eager: true, import: 'default' }) as Record<string, Record<string, unknown>>;
  let locale: Record<string, unknown> = localeModules['../../app/locale/lang/english.json'] ?? {};
  const languages = ['english', 'brazilian', 'czech', 'french', 'german', 'hungarian', 'italian', 'japanese', 'latam', 'polish', 'portuguese', 'russian', 'schinese', 'slovak', 'spanish', 'thai', 'turkish', 'ukrainian'];

  function t(path: string, fallback: string) {
    let value: unknown = locale;
    for (const part of path.split('.')) value = typeof value === 'object' && value ? (value as Record<string, unknown>)[part] : undefined;
    return typeof value === 'string' ? value : fallback;
  }

  function applyLanguage(language: string) {
    locale = localeModules[`../../app/locale/lang/${language}.json`] ?? localeModules['../../app/locale/lang/english.json'] ?? {};
    document.documentElement.lang = language === 'schinese' ? 'zh-CN' : language === 'brazilian' ? 'pt-BR' : language;
  }

  async function changeLanguage() {
    if (!settings) return;
    applyLanguage(settings.language);
    await save();
  }

  function toggleMaximize() {
    void appWindow.toggleMaximize();
  }

  function sourceIcon(source?: SourceKind) {
    if (source === 'rpcs3') return playstationIcon;
    if (source === 'steam' || source === 'steam_emulator' || source === 'green_luma') return steamIcon;
    if (source === 'epic') return epicIcon;
    if (source === 'gog') return gogIcon;
    return null;
  }

  function sourceMark(source?: SourceKind) {
    if (source === 'green_luma') return 'GL';
    if (!source) return 'DB';
    return source.slice(0, 2).toUpperCase();
  }

  function hasSteamAppId(game: GameSummary) {
    return /^\d+$/.test(game.gameId)
      && ['steam', 'steam_emulator', 'green_luma', 'watchdog_cache'].includes(game.sourceKind ?? '');
  }

  function imageUrl(value: string) {
    return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') ? convertFileSrc(value) : value;
  }

  function gameArtwork(game: GameSummary) {
    if (settings?.thumbnailPortrait && hasSteamAppId(game)) {
      return `https://cdn.cloudflare.steamstatic.com/steam/apps/${game.gameId}/library_600x900_2x.jpg`;
    }
    return game.icon ? imageUrl(game.icon) : undefined;
  }

  function gameArtworkFailed(event: Event, game: GameSummary) {
    const image = event.currentTarget as HTMLImageElement;
    if (settings?.thumbnailPortrait && game.icon && image.src !== imageUrl(game.icon)) image.src = imageUrl(game.icon);
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

  function totalUnlocked() {
    return games.reduce((total, game) => total + game.unlocked, 0);
  }

  function completedGames() {
    return games.filter((game) => game.total > 0 && game.unlocked === game.total).length;
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

  function achievementRows(achieved: boolean) {
    const term = achievementQuery.trim().toLowerCase();
    return achievements.filter((achievement) => achievement.achieved === achieved
      && (achieved || !achievement.hidden || settings?.showHidden || revealHiddenForGame)
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

  function formatPlaytime(seconds: number) {
    if (seconds < 60) return 'less than a minute';
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return hours ? `${hours}h ${minutes}m` : `${minutes} minutes`;
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
    gameMenu = null;
    achievementQuery = '';
    unlockedCollapsed = false;
    lockedCollapsed = false;
    highlightedAchievement = achievementId;
    revealHiddenForGame = false;
    selectedGame = game;
    achievements = [];
    achievementStatus = 'Loading achievements…';
    try {
      gameSourceChoices = await invoke<typeof gameSourceChoices>('game_sources', { gameId: game.gameId });
      activeAchievementSource = gameSourceChoices.some((choice) => choice.sourceId === game.sourceId)
        ? game.sourceId : (gameSourceChoices[0]?.sourceId ?? game.sourceId);
      achievements = await invoke<AchievementObservation[]>('list_achievements', {
        sourceId: activeAchievementSource,
        gameId: game.gameId,
      });
      achievementStatus = achievements.length === 0 ? 'No achievements were read from this source.' : '';
      if (achievementId) {
        await tick();
        document.querySelector(`[data-achievement-id="${CSS.escape(achievementId)}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    } catch (error) {
      achievementStatus = `Could not load achievements: ${String(error)}`;
    }
  }

  async function changeAchievementSource() {
    if (!selectedGame) return;
    achievementStatus = 'Loading achievements…';
    try {
      achievements = await invoke<AchievementObservation[]>('list_achievements', {
        sourceId: activeAchievementSource,
        gameId: selectedGame.gameId,
      });
      achievementStatus = achievements.length ? '' : 'No achievements were read from this source.';
    } catch (error) {
      achievements = [];
      achievementStatus = `Could not load this source: ${String(error)}`;
    }
  }

  function kindForSource(sourceId?: string) {
    return gameSourceChoices.find((choice) => choice.sourceId === sourceId)?.sourceKind;
  }

  function activeSourceKind() {
    return kindForSource(activeAchievementSource) ?? selectedGame?.sourceKind;
  }

  async function testNotification() {
    try {
      await invoke('test_notification');
      status = 'Test notification sent';
    } catch (error) {
      status = `Notification test failed: ${String(error)}`;
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
    } catch (error) {
      status = `Scan failed: ${String(error)}`;
    } finally {
      scanning = false;
    }
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
    if (await save()) await scan(true);
  }

  async function save() {
    if (!settings) return false;
    try {
      await invoke('save_settings', { settings });
      await refresh();
      status = 'Settings saved';
      return true;
    } catch (error) {
      status = `Could not save settings: ${String(error)}`;
      return false;
    }
  }

  function openSettings() {
    if (settings) settingsSnapshot = structuredClone(settings);
    settingsTab = 'general';
    view = 'settings';
  }

  async function acceptSettings() {
    if (!(await save())) return;
    settingsSnapshot = null;
    view = 'library';
  }

  async function cancelSettings() {
    if (settingsSnapshot) {
      settings = structuredClone(settingsSnapshot);
      applyLanguage(settings.language);
      await save();
    }
    settingsSnapshot = null;
    view = 'library';
    status = 'Settings changes cancelled';
  }

  async function removeSource(id: string) {
    if (!settings) return;
    settings.sourceLocations = settings.sourceLocations.filter((source) => source.id !== id);
    await save();
  }

  async function detectSources(deep = false, scanAfter = true) {
    if (!settings) return;
    status = deep ? 'Searching local drives for achievement sources…' : status;
    try {
      const detected = await invoke<AppSettings['sourceLocations']>('detect_sources', { deep });
      const known = new Set(settings.sourceLocations.map((source) => source.path.toLowerCase()));
      const additions = detected.filter((source) => !known.has(source.path.toLowerCase()));
      settings.sourceLocations = [...settings.sourceLocations, ...additions];
      settings.sourcesInitialized = true;
      if (await save()) {
        status = additions.length ? `Found ${additions.length} achievement folder${additions.length === 1 ? '' : 's'}` : 'No new achievement folders found';
        if (scanAfter && additions.length) await scan(true);
      }
    } catch (error) {
      status = `Source discovery failed: ${String(error)}`;
    }
  }

  async function chooseScreenshotFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && settings) {
      settings.screenshotDirectory = selected;
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

  async function refreshMissingMetadata() {
    try {
      const updated = await invoke<number>('refresh_metadata', { gameId: null });
      await refresh();
      status = updated > 0
        ? `Library information updated for ${updated} item${updated === 1 ? '' : 's'}`
        : 'Library is up to date';
    } catch (error) {
      // Metadata is optional enrichment. A network or Steam Community failure
      // must not turn an otherwise usable local library into a startup failure.
      status = `Some game information could not be refreshed: ${String(error)}`;
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

  async function exportGoldbergAchievements(game: GameSummary) {
    gameMenu = null;
    const path = await saveDialog({
      title: 'Generate achievements.json for Goldberg Steam Emulator',
      defaultPath: 'achievements.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!path) return;
    status = `Exporting achievements for ${game.name}…`;
    try {
      const count = await invoke<number>('export_goldberg_achievements', {
        sourceId: game.sourceId, gameId: game.gameId, path,
      });
      status = `Exported ${count} achievements for ${game.name}`;
    } catch (error) { status = `Goldberg export failed: ${String(error)}`; }
  }

  async function resetGameActivity(game: GameSummary) {
    gameMenu = null;
    try {
      await invoke('reset_game_activity', { gameId: game.gameId });
      await refresh();
      status = `Reset playtime and last played for ${game.name}`;
    } catch (error) {
      status = `Could not reset ${game.name}: ${String(error)}`;
    }
  }

  async function blacklistGame(game: GameSummary) {
    gameMenu = null;
    if (!settings || settings.blacklistedGameIds.includes(game.gameId)) return;
    settings.blacklistedGameIds = [...settings.blacklistedGameIds, game.gameId];
    if (selectedGame?.gameId === game.gameId) selectedGame = null;
    await save();
  }

  function configureGame(game: GameSummary) {
    gameMenu = null;
    const config = settings?.gameLaunchConfigs[game.gameId];
    gameConfig = { game, executable: config?.executable ?? '', arguments: config?.arguments ?? '' };
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
    try {
      avatarData = await invoke<string>('read_profile_avatar', { path: selected });
      settings.profileAvatarPath = selected;
      await save();
    } catch (error) { status = `Could not use avatar: ${String(error)}`; }
  }

  async function loadAvatar() {
    if (!settings?.profileAvatarPath) { avatarData = ''; return; }
    avatarData = await invoke<string>('read_profile_avatar', { path: settings.profileAvatarPath }).catch(() => '');
  }

  async function resetAvatar() {
    avatarMenu = null;
    if (!settings) return;
    settings.profileAvatarPath = undefined;
    avatarData = '';
    await save();
  }

  async function importSteamAvatar(account: { steamId: string; name: string }) {
    avatarMenu = null;
    if (!settings) return;
    status = `Importing ${account.name || 'Steam'} avatar…`;
    try {
      const path = await invoke<string>('import_steam_avatar', { steamId: account.steamId });
      settings.profileAvatarPath = path;
      await loadAvatar();
      await save();
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
    status = 'Checking for preview updates…';
    try {
      availableUpdate = await invoke<UpdateInfo | null>('check_for_updates', { manual });
      status = availableUpdate ? `Achievement Watcher ${availableUpdate.version} is available` : 'Achievement Watcher is up to date';
    } catch (error) {
      status = `Update check failed: ${String(error)}`;
    }
  }

  async function skipUpdate() {
    if (!settings || !availableUpdate) return;
    settings.skippedUpdateVersion = availableUpdate.version;
    availableUpdate = null;
    await save();
    status = 'This preview version will be skipped';
  }

  async function installAvailableUpdate() {
    installingUpdate = true;
    status = 'Downloading and verifying the update installer…';
    try { await invoke('install_update'); }
    catch (error) { installingUpdate = false; status = `Update installation failed: ${String(error)}`; }
  }

  async function saveGameConfig() {
    if (!settings || !gameConfig || !gameConfig.executable) return;
    settings.gameLaunchConfigs = { ...settings.gameLaunchConfigs, [gameConfig.game.gameId]: {
      executable: gameConfig.executable, arguments: gameConfig.arguments,
    } };
    if (await save()) gameConfig = null;
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
    if (gameMenu || avatarMenu) {
      gameMenu = null;
      avatarMenu = null;
    } else if (gameConfig) {
      gameConfig = null;
    } else if (view === 'settings') {
      void cancelSettings();
    } else if (selectedGame) {
      selectedGame = null;
    }
  }

  onMount(() => {
    let disposed = false;
    const cleanup: Array<() => void> = [];
    void (async () => {
      cleanup.push(await listen('library-changed', refresh));
      cleanup.push(await listen<{ transport: string; success: boolean; error?: string }>('notification-status', ({ payload }) => {
        status = payload.success
          ? `${payload.transport === 'overlay' ? 'Custom popup rendered' : 'Windows notification delivered'}`
          : `Notification failed: ${payload.error ?? 'unknown error'}`;
      }));
      cleanup.push(await listen<{ completed: number; total: number }>('scan-progress', ({ payload }) => {
        status = `Scanning ${payload.completed} of ${payload.total}`;
      }));
      cleanup.push(await listen<OpenGameRequest>('open-game', ({ payload }) => {
        void consumeOpenGameRequest(payload);
      }));
      await consumeOpenGameRequest();
      try {
        await invoke('import_legacy');
        settings = await invoke<AppSettings>('load_settings');
        await loadDiagnostics();
        applyLanguage(settings.language);
        await loadAvatar();
        await refresh();
        status = games.length ? 'Refreshing library…' : 'Searching for achievement data…';
        await detectSources(false, false);
        steamAccounts = await invoke<typeof steamAccounts>('steam_accounts');
        await scan(true);
        status = 'Library is ready; checking missing game information…';
        void refreshMissingMetadata();
        void checkUpdates(false);
      } catch (error) {
        status = `Startup failed: ${String(error)}`;
      }
      if (disposed) cleanup.splice(0).forEach((unlisten) => unlisten());
    })();
    return () => {
      disposed = true;
      cleanup.splice(0).forEach((unlisten) => unlisten());
    };
  });
</script>

<svelte:head><title>Achievement Watcher</title></svelte:head>
<svelte:window onclick={() => { gameMenu = null; avatarMenu = null; }} onkeydown={handleWindowKeydown} />

<header class="title-bar" data-tauri-drag-region>
  <div class="watcher-state"><span class:busy={scanning}></span><span>{scanning ? 'Scanning achievements…' : 'Achievement Watcher is running'}</span></div>
  <nav aria-label="Window controls">
    <button aria-label="Minimize" title="Minimize" onclick={() => appWindow.minimize()}><i class="far fa-window-minimize"></i></button>
    <button aria-label="Settings" title="Settings" class:active={view === 'settings'} disabled={view === 'settings'} onclick={openSettings}><i class="fas fa-cog"></i></button>
    <button aria-label="Maximize" title="Maximize" onclick={toggleMaximize}><i class="far fa-window-maximize"></i></button>
    <button class="close" aria-label="Close" title="Close" onclick={() => appWindow.close()}><i class="fas fa-times"></i></button>
  </nav>
</header>

<main>
  {#if view === 'library'}
  {#if selectedGame}
    <section id="achievement" aria-label={`${selectedGame.name} achievements`}>
      {#if selectedGame.icon}<img class="game-background" src={imageUrl(selectedGame.icon)} alt="" />{/if}
      <div class="achievement-page-header">
        <div class="game-heading">
          <div class="source-badge large" title={activeAchievementSource === 'merged' ? 'Merged from every enabled source' : sourceDescription(activeSourceKind())}>{#if sourceIcon(activeSourceKind())}<img src={sourceIcon(activeSourceKind())!} alt="" />{:else}{sourceMark(activeSourceKind())}{/if}</div>
          <div><h2>{selectedGame.name}</h2>{#if gameSourceChoices.length > 1}<select class="game-source-select" bind:value={activeAchievementSource} onchange={changeAchievementSource} aria-label="Achievement source">{#each gameSourceChoices as choice}<option value={choice.sourceId}>{choice.sourceId === 'merged' ? 'Merged achievement sources' : `${sourceLabel(choice.sourceKind)} — ${choice.sourceId}`}</option>{/each}</select>{:else}<span>{selectedGame.sourceId === 'merged' ? 'Merged achievement sources' : sourceLabel(selectedGame.sourceKind)}</span>{/if}</div>
        </div>
        <div class="game-activity"><div class="achievement-summary"><strong>{detailUnlocked()} / {achievements.length}</strong><span>{achievements.length ? Math.round(detailUnlocked() / achievements.length * 100) : 0}%</span></div>{#if achievements.some((achievement) => achievement.trophyGrade)}<ul class="detail-trophies trophy-totals" aria-label="Unlocked trophies"><li class="platinum" title="Platinum trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('platinum')}</li><li class="gold" title="Gold trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('gold')}</li><li class="silver" title="Silver trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('silver')}</li><li class="bronze" title="Bronze trophies"><i class="fas fa-trophy"></i> {achievementTrophyTotal('bronze')}</li></ul>{/if}{#if selectedGame.playtimeSeconds > 0}<div class="activity-pill" title="Tracked playtime"><i class="fas fa-gamepad"></i> {formatPlaytime(selectedGame.playtimeSeconds)}</div>{/if}{#if selectedGame.lastPlayed > 0}<div class="activity-pill"><i class="far fa-clock"></i> {new Date(selectedGame.lastPlayed * 1000).toLocaleDateString()}</div>{/if}</div>
        <button class="back-button" aria-label="Back to games" title="Back to games" onclick={() => selectedGame = null}><i class="fas fa-chevron-left"></i></button>
      </div>
      <div class="achievement-tools"><div id="achievement-search"><span><i class="fas fa-search"></i></span><input class:has={achievementQuery.length > 0} type="search" bind:value={achievementQuery} placeholder="Search achievements" aria-label="Search achievements" /></div></div>
      {#if achievementStatus}<p class="detail-status">{achievementStatus}</p>{/if}
      {#each [['Unlocked', true], ['Locked', false]] as group}
        {@const rows = achievementRows(group[1] as boolean)}
        {@const collapsed = group[1] ? unlockedCollapsed : lockedCollapsed}
        {@const groupCount = achievements.filter((achievement) => achievement.achieved === (group[1] as boolean)).length}
        {#if rows.length || group[1] || hiddenLockedCount() > 0}
          <section class="achievement-group">
            <h3><span><i class={group[1] ? 'fas fa-unlock' : 'fas fa-lock'}></i> {group[0]} <small>{groupCount}</small></span><span class="achievement-sort"><button class:active={achievementSort === 'name'} title="Sort achievements alphabetically" aria-label="Sort achievements alphabetically" onclick={() => achievementSort = 'name'}><i class="fas fa-sort-alpha-down"></i></button>{#if group[1]}<button class:active={achievementSort === 'time'} title="Sort by unlock time" aria-label="Sort by unlock time" onclick={() => achievementSort = 'time'}><i class="far fa-clock"></i></button>{/if}{#if !group[1]}<button class:active={achievementSort === 'progress'} title="Sort by progress" aria-label="Sort by progress" onclick={() => achievementSort = 'progress'}><i class="fas fa-percent"></i></button>{/if}<button class:active={achievementSort === 'rarity'} title="Sort by global rarity" aria-label="Sort by global rarity" onclick={() => achievementSort = 'rarity'}><i class="fas fa-gem"></i></button></span><button class="collapse-toggle" class:active={!collapsed} aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${group[0]} achievements`} onclick={() => group[1] ? unlockedCollapsed = !unlockedCollapsed : lockedCollapsed = !lockedCollapsed}><i class="fas fa-chevron-right"></i></button></h3>
            {#if !collapsed}<ul>
              {#if group[1] && rows.length === 0}<li class="achievement-notice"><i class="fas fa-frown-open"></i><strong>{t('noneUnlocked', 'No achievement unlocked yet')}</strong><span>{t('play', 'Start playing!')}</span></li>{/if}
              {#each rows as achievement}
                <li><article data-achievement-id={achievement.achievementId} class:highlight={highlightedAchievement === achievement.achievementId} class:unlocked={achievement.achieved} class:rare={(achievement.globalPercentHundredths ?? 10_001) <= 1000} class="achievement-row">
                  <div class="achievement-icon"><span><i class={achievement.achieved ? 'fas fa-trophy' : 'fas fa-lock'}></i></span>{#if achievement.icon}<img src={imageUrl(achievement.icon)} alt="" onerror={(event) => event.currentTarget.remove()} />{/if}</div>
                  <div class="achievement-content"><h4>{achievement.displayName ?? achievement.achievementId}</h4><p>{achievement.hidden && !achievement.achieved && !settings?.showHidden ? t('revealedOnceUnlocked', 'Details for this achievement will be revealed once unlocked') : (achievement.description ?? 'No description available.')}</p>{#if !achievement.achieved && achievement.maxProgress > 0}<div class="achievement-progress"><i style={`width:${Math.min(100, achievement.currentProgress / achievement.maxProgress * 100)}%`}></i><span>{achievement.currentProgress} / {achievement.maxProgress}</span></div>{/if}</div>
                  <div class="achievement-state">{#if achievement.originSourceId}<i class="achievement-origin source-badge" title={`State from ${sourceDescription(kindForSource(achievement.originSourceId))}`}>{#if sourceIcon(kindForSource(achievement.originSourceId))}<img src={sourceIcon(kindForSource(achievement.originSourceId))!} alt="" />{:else}{sourceMark(kindForSource(achievement.originSourceId))}{/if}</i>{/if}{#if achievement.trophyGrade}<i class={`trophy-grade ${achievement.trophyGrade} fas fa-trophy`} title={`${achievement.trophyGrade} trophy`}></i>{/if}{#if achievement.achieved}<strong>{t('unlocked', 'Unlocked')}</strong>{#if achievement.unlockTime > 0}<time title={new Date(achievement.unlockTime * 1000).toLocaleString()}>{formatUnlockTime(achievement.unlockTime)}</time>{/if}{:else}<span>{t('locked', 'Locked')}</span>{/if}{#if achievement.globalPercentHundredths !== undefined}<small title="Global Steam unlock percentage"><i class="fas fa-gem"></i> {(achievement.globalPercentHundredths / 100).toFixed(2)}% {t('globalStat', 'of players have this')}</small>{/if}</div>
                </article></li>
              {/each}
            </ul>{#if !group[1] && !settings?.showHidden && !revealHiddenForGame && hiddenLockedCount() > 0}<div class="hidden-disclaimer"><span><i class="fas fa-eye-slash"></i> {hiddenLockedCount()} {t('hiddenRemain', 'hidden achievements remaining')}</span><button onclick={() => revealHiddenForGame = true}>{t('settings.common.show', 'Show')} hidden achievements</button></div>{/if}{/if}
          </section>
        {/if}
      {/each}
      <button class="scroll-top" title="Scroll to top" aria-label="Scroll to top" onclick={() => document.getElementById('achievement')?.scrollTo({ top: 0, behavior: 'smooth' })}><i class="fas fa-chevron-up"></i></button>
    </section>
  {:else}
    <section id="home" aria-labelledby="library-title">
      <div id="user-info"><button class="avatar" class:squared={settings?.profileAvatarSquared} title="Choose profile avatar (right-click for options)" aria-label="Choose profile avatar" onclick={chooseAvatar} oncontextmenu={(event) => { event.preventDefault(); event.stopPropagation(); avatarMenu = { x: event.clientX, y: event.clientY }; }}><img src={avatarData || defaultAvatar} alt="" /></button><div class="info"><h1>{settings?.username || 'Achievement Watcher'}</h1><ul><li><i class="fas fa-trophy"></i> <strong>{totalUnlocked()}</strong> unlocked</li><li><i class="fas fa-gamepad"></i> <strong>{completedGames()}/{games.length}</strong> games completed</li><li><i class="fas fa-cookie-bite"></i> <strong>{averageCompletion()}%</strong> average</li></ul>{#if trophyTotal('platinum') + trophyTotal('gold') + trophyTotal('silver') + trophyTotal('bronze') > 0}<ul class="trophy-totals" aria-label="PlayStation trophies"><li class="platinum" title="Platinum trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('platinum')}</strong></li><li class="gold" title="Gold trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('gold')}</strong></li><li class="silver" title="Silver trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('silver')}</strong></li><li class="bronze" title="Bronze trophies"><i class="fas fa-trophy"></i> <strong>{trophyTotal('bronze')}</strong></li></ul>{/if}</div></div>
      <div class="library-tools"><div id="search-bar"><span><i class="fas fa-search"></i></span><input class:has={query.length > 0} type="search" bind:value={query} placeholder="Search games" aria-label="Search games" /></div><select bind:value={libraryFilter} aria-label="Filter games"><option value="all">All games</option><option value="tracked">Tracked</option><option value="cached">Cached information</option></select><button class="refresh" title="Refresh library" aria-label="Refresh library" onclick={() => scan(false)} disabled={scanning}><i class="fas fa-sync-alt"></i></button><div id="sort-box" aria-label="Sort games"><button class:active={librarySort === 'name'} title="Sort alphabetically" aria-label="Sort alphabetically" onclick={() => librarySort = 'name'}><i class="fas fa-sort-alpha-down"></i></button><button class:active={librarySort === 'progress'} title="Sort by completion" aria-label="Sort by completion" onclick={() => librarySort = 'progress'}><i class="fas fa-sort-numeric-down"></i><i class="fas fa-percent"></i></button><button class:active={librarySort === 'recent'} title="Sort by most recent unlock" aria-label="Sort by most recent unlock" onclick={() => librarySort = 'recent'}><i class="fas fa-sort-numeric-down"></i><i class="far fa-clock"></i></button></div></div>
      <div id="game-list" class:view-portrait={settings?.thumbnailPortrait}>
{#if games.length === 0}<div class="empty"><strong>No games found</strong><span>Achievement folders are detected automatically. Check Settings if your folder is missing.</span><button onclick={openSettings}>Open settings</button></div>{:else}<ul>{#each visibleGames() as game}<li><div class="game-box" role="button" tabindex="0" onclick={() => openGame(game)} onkeydown={(event) => { if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); void openGame(game); } }} oncontextmenu={(event) => { event.preventDefault(); event.stopPropagation(); gameMenu = { game, x: event.clientX, y: event.clientY }; }} title={`${game.name} — ${game.sourceId === 'merged' ? 'Merged from every enabled source' : sourceDescription(game.sourceKind)}`}><div class="game-header"><span>{game.name.slice(0, 1).toUpperCase()}</span>{#if gameArtwork(game)}<img src={gameArtwork(game)} alt="" onerror={(event) => gameArtworkFailed(event, game)} />{/if}<button class="achievement-button" title={`View ${game.name} achievements`} aria-label={`View ${game.name} achievements`} onclick={(event) => { event.stopPropagation(); void openGame(game); }}><i class="fas fa-trophy"></i></button><button class="play-button" title={`Play ${game.name}`} aria-label={`Play ${game.name}`} onclick={(event) => { event.stopPropagation(); void launchGame(game); }}><i class="fas fa-play"></i></button><button class="config-button" title={`Configure ${game.name}`} aria-label={`Configure ${game.name}`} onclick={(event) => { event.stopPropagation(); configureGame(game); }}><i class="fas fa-tools"></i></button></div><div class="game-info"><div><strong>{game.name}</strong><i class="source-badge" title={game.sourceId === 'merged' ? 'Merged from every enabled source' : sourceDescription(game.sourceKind)}>{#if sourceIcon(game.sourceKind)}<img src={sourceIcon(game.sourceKind)!} alt="" />{:else}{sourceMark(game.sourceKind)}{/if}</i></div><div class="game-progress" data-percent={Math.round(completionPercent(game))}><i style={`width:${completionPercent(game)}%`}></i></div></div></div></li>{/each}</ul>{/if}
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
        {#each [['general','fas fa-tools',t('settings.sideMenu.general','General')],['notification','fas fa-bell',t('settings.sideMenu.notification','Notification')],['souvenir','fas fa-camera',t('settings.sideMenu.souvenir','Souvenir')],['folder','far fa-folder',t('settings.sideMenu.folder','Folder')],['source','fas fa-file-import',t('settings.sideMenu.source','Source')],['advanced','fas fa-flask',t('settings.sideMenu.advanced','Advanced')],['debug','fas fa-bug',t('settings.sideMenu.debug','Debug')]] as tab}
          <button class:active={settingsTab === tab[0]} onclick={() => settingsTab = tab[0] as typeof settingsTab}><i class={tab[1]}></i>{tab[2]}</button>
        {/each}
      </nav>
      <div class="settings-content">
      {#if settingsTab === 'general'}
      <div class="settings-group">
        <h3>Interface and library</h3>
        <label class="field"><span>{t('settings.general.language.name', 'Language')}</span><select bind:value={settings.language} onchange={changeLanguage}>{#each languages as language}<option value={language}>{language[0].toUpperCase() + language.slice(1)}</option>{/each}</select></label>
        <label class="field"><span>Username</span><input bind:value={settings.username} onchange={save} placeholder="Windows account name" /></label>
        <div class="field"><span>Profile avatar</span><button onclick={chooseAvatar}>{settings.profileAvatarPath ? 'Change' : 'Choose'}</button>{#if settings.profileAvatarPath}<button onclick={() => { if (settings) { settings.profileAvatarPath = undefined; avatarData = ''; void save(); } }}>Remove</button>{/if}</div>
        <label class="field"><span>Game thumbnails</span><select bind:value={settings.thumbnailPortrait} onchange={save}><option value={false}>Landscape</option><option value={true}>Portrait</option></select></label>
        <label class="check"><input type="checkbox" bind:checked={settings.showCachedGames} onchange={save} /> Show games that have cached information but no tracked progress</label>
        <label class="check"><input type="checkbox" bind:checked={settings.hideZero} onchange={save} /> Hide games with no unlocked achievements</label>
        <label class="check"><input type="checkbox" bind:checked={settings.showHidden} onchange={save} /> Reveal hidden achievement names and descriptions</label>
        <label class="check"><input type="checkbox" bind:checked={settings.mergeDuplicate} onchange={save} /> Merge the same game when it is found in multiple sources</label>
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
        <h3>Updates</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.checkForUpdates} onchange={save} /> Check for new preview releases automatically</label>
        <div class="field"><span>Application updates</span><button onclick={() => checkUpdates(true)}>Check now</button></div>
        <p class="settings-help">Updates use the regular NSIS installer. Downloaded installers are SHA-256 verified before they are opened.</p>
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
        {#if settings.gameBarEnabled}<label class="check nested"><input type="checkbox" bind:checked={settings.gameBarFullscreenOnly} onchange={save} /> Use the companion only while a fullscreen app is active</label><div class="token-row"><span>Pairing token</span><code>{settings.gameBarToken}</code><button onclick={() => invoke('test_game_bar').then(() => status = 'Game Bar acknowledged the test').catch((error) => status = `Game Bar test failed: ${String(error)}`)}>Test</button></div>{/if}
      </div>
      <div class="settings-group">
        <h3>In-game achievement overlay</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.achievementOverlayEnabled} onchange={save} /> Show the current game's achievement list over a running game</label>
        {#if settings.achievementOverlayEnabled}<label class="field"><span>Toggle shortcut</span><input bind:value={settings.achievementOverlayHotkey} onchange={save} aria-label="Achievement overlay shortcut" /></label><label class="field"><span>Overlay scale</span><input type="range" min="50" max="200" step="5" bind:value={settings.achievementOverlayScalePercent} onchange={save} /><output>{settings.achievementOverlayScalePercent}%</output></label><div class="field"><span>Current game</span><button onclick={() => invoke('toggle_achievement_overlay').then(() => status = 'Achievement overlay toggled').catch((error) => status = `Overlay unavailable: ${String(error)}`)}>Toggle overlay</button></div>{/if}
        <p class="settings-help">The overlay is created only while visible and closes completely when toggled off. Exclusive-fullscreen games may require the Xbox Game Bar companion.</p>
      </div>
      {:else if settingsTab === 'souvenir'}
      <div class="settings-group">
        <h3>Screenshot</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.screenshotEnabled} onchange={save} /> Save a screenshot when an achievement unlocks</label>
        {#if settings.screenshotEnabled}<label class="check nested"><input type="checkbox" bind:checked={settings.screenshotOverwrite} onchange={save} /> Replace an existing screenshot for the same achievement</label><div class="folder-setting"><span>Save location</span><code>{settings.screenshotDirectory ?? 'Application data / screenshots'}</code><button onclick={() => invoke('open_data_location', { location: 'screenshots' }).catch((error) => status = String(error))}>Open</button><button onclick={chooseScreenshotFolder}>Choose</button>{#if settings.screenshotDirectory}<button onclick={() => { if (settings) { settings.screenshotDirectory = undefined; void save(); } }}>Reset</button>{/if}</div>{/if}
      </div>
      <div class="settings-group">
        <h3>OBS replay buffer</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.obsReplayEnabled} onchange={save} /> Save an OBS replay when an achievement unlocks</label>
        {#if settings.obsReplayEnabled}
          <p class="settings-help">OBS is optional and must have its WebSocket server enabled. If OBS is closed or unavailable, notifications and screenshots continue normally.</p>
          <label class="field"><span>Host</span><input bind:value={settings.obsHost} onchange={save} /></label>
          <label class="field"><span>Port</span><input type="number" min="1" max="65535" bind:value={settings.obsPort} onchange={save} /></label>
          <label class="field"><span>Password</span><input type="password" bind:value={settings.obsPassword} onchange={save} autocomplete="off" /></label>
          <label class="check nested"><input type="checkbox" bind:checked={settings.obsStartReplayBuffer} onchange={save} /> Start the replay buffer if OBS has not started it</label>
          <div class="field"><span>Connection and replay</span><button onclick={() => invoke('test_obs').then(() => status = 'OBS replay buffer saved').catch((error) => status = `OBS test failed: ${String(error)}`)}>Run test</button></div>
        {/if}
      </div>
      {:else if settingsTab === 'folder'}
      <div class="settings-group">
        <h3>Achievement folders</h3>
        <div class="sources">
          {#each settings.sourceLocations as source}
            <div class="source"><label><input type="checkbox" bind:checked={source.enabled} onchange={save} />{sourceLabel(source.kind)}</label><label title="Allow unlock notifications and souvenir actions from this folder"><input type="checkbox" bind:checked={source.notify} onchange={save} disabled={!source.enabled} />Notify</label><code title={source.path}>{source.path}</code><button aria-label={`Remove ${source.path}`} onclick={() => removeSource(source.id)}>Remove</button></div>
          {:else}<p class="muted">No live achievement folders were detected. Cached games can still be browsed, but unlocks are not monitored.</p>{/each}
        </div>
        <div class="source-actions"><button onclick={() => detectSources(true, true)}>Smart Find</button><button onclick={() => addSource('steam_emulator')}>Add Steam emulator folder</button><button onclick={() => addSource('rpcs3')}>Add RPCS3 folder</button><button onclick={() => addSource('epic')}>Add Epic emulator folder</button><button onclick={() => addSource('gog')}>Add GOG emulator folder</button></div>
      </div>
      {:else if settingsTab === 'source'}
      <div class="settings-group">
        <h3>Official Steam</h3>
        <label class="check"><input type="checkbox" bind:checked={settings.steamEnabled} onchange={save} /> Import achievements from the signed-in Steam client</label>
        {#if settings.steamEnabled}<label class="field"><span>Games to display</span><select bind:value={settings.steamLibraryMode} onchange={save}><option value="played">Games with local Steam stats</option><option value="installed">Installed</option><option value="owned">Owned (public profile or API key)</option></select></label><label class="check"><input type="checkbox" bind:checked={settings.steamPublicFallback} onchange={save} /> Use the public Steam profile when client data is unavailable</label>{#if steamAccounts.length}<label class="field"><span>Steam account</span><select bind:value={settings.steamAccountId} onchange={save}><option value={undefined}>Most recently used account</option>{#each steamAccounts as account}<option value={account.accountId}>{account.name || account.steamId}{account.mostRecent ? ' (recent)' : ''}</option>{/each}</select></label>{:else}<label class="field"><span>Steam account</span><input bind:value={settings.steamAccountId} onchange={save} placeholder="Detected automatically" /></label>{/if}{/if}
      </div>
      <div class="settings-group">
        <h3>Local achievement sources</h3>
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
        <h3>Steam Web API</h3>
        <p class="settings-help">Optional fallback for profiles that cannot be read through the signed-in Steam client. Leave blank for local-only operation.</p>
        <label class="field"><span>Web API key</span><input type="password" bind:value={settings.steamApiKey} onchange={save} autocomplete="off" placeholder="Optional" /></label>
      </div>
      <div class="settings-group">
        <h3>Blacklist</h3>
        <div class="field"><span>{settings.blacklistedGameIds.length} hidden game{settings.blacklistedGameIds.length === 1 ? '' : 's'}</span><button disabled={settings.blacklistedGameIds.length === 0} onclick={() => { if (settings) { settings.blacklistedGameIds = []; void save(); } }}>Clear blacklist</button></div>
      </div>
      <div class="settings-group">
        <h3>Notification filtering</h3>
        <label class="field"><span>Unlock timestamp threshold</span><input type="number" min="0" max="3600" bind:value={settings.notificationMaxAgeSeconds} onchange={save} /><small>seconds</small></label>
        <label class="check"><input type="checkbox" bind:checked={settings.notificationRequireRunningGame} onchange={save} /> Require the configured game executable or a fullscreen app to be running</label>
        <p class="settings-help">Games without a configured executable continue to notify. Set the threshold to 0 to accept only unlocks stamped at the current second.</p>
      </div>
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
        {#if settings.gntpEnabled}<label class="field"><span>Host</span><input bind:value={settings.gntpHost} onchange={save} /></label><label class="field"><span>Port</span><input type="number" min="1" max="65535" bind:value={settings.gntpPort} onchange={save} /></label><div class="field"><span>Connection</span><button onclick={() => invoke('test_gntp').then(() => status = 'GNTP test sent').catch((error) => status = `GNTP test failed: ${String(error)}`)}>Run test</button></div>{/if}
      </div>
      {:else}
      <div class="settings-group">
        <h3>Diagnostics</h3>
        <div class="field"><span>Achievement notification</span><button onclick={testNotification}>Run test</button></div>
        <div class="field"><span>Progress notification</span><button onclick={() => invoke('test_progress_notification').then(() => status = 'Progress notification test sent').catch((error) => status = `Progress test failed: ${String(error)}`)}>Run test</button></div>
        <div class="field"><span>Playtime notification</span><button onclick={() => invoke('test_playtime_notification').then(() => status = 'Playtime notification test sent').catch((error) => status = `Playtime test failed: ${String(error)}`)}>Run test</button></div>
        <div class="field"><span>Windows notification controls</span><button onclick={() => invoke('open_windows_settings', { page: 'focus_assist' }).catch((error) => status = String(error))}>Focus Assist</button><button onclick={() => invoke('open_windows_settings', { page: 'notifications' }).catch((error) => status = String(error))}>Notifications &amp; actions</button></div>
        <p class="settings-help">Fullscreen Focus Assist rules can suppress native Windows notifications. Custom desktop popups and the optional Game Bar companion use separate delivery paths.</p>
        <div class="field"><span>Scan all sources</span><button onclick={() => scan(false)} disabled={scanning}>Run scan</button></div>
        {#if settings.gameBarEnabled}<div class="field"><span>Xbox Game Bar</span><button onclick={() => invoke('test_game_bar').then(() => status = 'Game Bar acknowledged the test').catch((error) => status = `Game Bar test failed: ${String(error)}`)}>Run test</button></div>{/if}
        {#if settings.obsReplayEnabled}<div class="field"><span>OBS replay buffer</span><button onclick={() => invoke('test_obs').then(() => status = 'OBS replay buffer saved').catch((error) => status = `OBS test failed: ${String(error)}`)}>Run test</button></div>{/if}
        <div class="field"><span>Runtime status</span><button onclick={loadDiagnostics}>Refresh</button></div>
        {#if diagnosticData}<div class="diagnostic-grid"><span>Version</span><strong>{diagnosticData.appVersion}</strong><span>Games</span><strong>{diagnosticData.gameCount}</strong><span>Achievement records</span><strong>{diagnosticData.observationCount}</strong><span>Enabled folders</span><strong>{diagnosticData.enabledSourceCount}</strong><span>Missing folders</span><strong class:warning={diagnosticData.missingSourceCount > 0}>{diagnosticData.missingSourceCount}</strong><span>Pending notifications</span><strong>{diagnosticData.pendingNotifications}</strong><span>Failed notifications</span><strong class:warning={diagnosticData.failedNotifications > 0}>{diagnosticData.failedNotifications}</strong></div>{#if diagnosticData.failedNotifications > 0}<div class="field"><span>Failed notification queue</span><button onclick={() => recoverFailedNotifications(false)}>Retry now</button><button onclick={() => recoverFailedNotifications(true)}>Dismiss</button></div>{/if}{#if diagnosticData.recentErrors.length}<div class="diagnostic"><span>Recent delivery errors</span>{#each diagnosticData.recentErrors as message}<code>{message}</code>{/each}</div>{/if}<div class="diagnostic"><span>Notification log</span><code>{diagnosticData.notificationLog}</code><button onclick={() => invoke('open_data_location', { location: 'notification_log' }).catch((error) => status = String(error))}>Open</button></div>{/if}
      </div>
      {/if}
      </div></div>
      <div class="settings-footer"><div class="settings-notice"><span>Preview v{diagnosticData?.appVersion ?? '…'} ·</span><button onclick={() => invoke('open_project_page', { project: 'fork' })}>darktakayanagi/achievement-watcher</button><span>· Original v1.6.8 ·</span><button onclick={() => invoke('open_project_page', { project: 'original' })}>xan105/achievement-watcher</button></div><div><button onclick={cancelSettings}>{t('settings.common.cancel', 'Cancel')}</button><button class="primary" onclick={acceptSettings}>{t('settings.common.save', 'Save')}</button></div></div>
      </div>
    </section>
  {/if}
  {/if}
</main>
{#if availableUpdate}
  <aside class="update-banner" aria-live="polite"><i class="fas fa-download"></i><div><strong>Achievement Watcher {availableUpdate.version} is available</strong><span>{availableUpdate.installerName}</span></div><button onclick={() => invoke('open_release_page', { url: availableUpdate!.releaseUrl })}>Release notes</button><button onclick={skipUpdate}>Skip</button><button class="primary" disabled={installingUpdate} onclick={installAvailableUpdate}>{installingUpdate ? 'Downloading…' : 'Install update'}</button></aside>
{/if}
{#if avatarMenu && settings}
  <div class="context-menu" style={`left:${Math.min(avatarMenu.x, innerWidth - 240)}px;top:${Math.min(avatarMenu.y, innerHeight - 240)}px`} role="menu" tabindex="-1" oncontextmenu={(event) => event.preventDefault()}>
    <div class="context-title">Profile avatar</div>
    <button role="menuitemcheckbox" aria-checked={settings.profileAvatarSquared} onclick={() => { if (settings) { settings.profileAvatarSquared = !settings.profileAvatarSquared; avatarMenu = null; void save(); } }}><i class={settings.profileAvatarSquared ? 'fas fa-check-square' : 'far fa-square'}></i> Squared</button>
    <button role="menuitem" onclick={() => { avatarMenu = null; void chooseAvatar(); }}><i class="fas fa-folder-open"></i> Browse…</button>
    <button role="menuitem" onclick={resetAvatar}><i class="fas fa-redo-alt"></i> Reset to default avatar</button>
    {#each steamAccounts as account}<button role="menuitem" onclick={() => importSteamAvatar(account)}><i class="fab fa-steam"></i> Import {account.name || account.steamId}'s Steam avatar</button>{/each}
  </div>
{/if}
{#if gameMenu}
  <div class="context-menu" style={`left:${Math.min(gameMenu.x, innerWidth - 210)}px;top:${Math.max(8, Math.min(gameMenu.y, innerHeight - 450))}px`} role="menu" tabindex="-1" oncontextmenu={(event) => event.preventDefault()}>
    <div class="context-title">{gameMenu.game.name}</div>
    <button role="menuitem" onclick={() => launchGame(gameMenu!.game)}>Play</button>
    <button role="menuitem" onclick={() => openGame(gameMenu!.game)}>View achievements</button>
    <button role="menuitem" onclick={() => configureGame(gameMenu!.game)}>Configure executable</button>
    <button role="menuitem" onclick={() => refreshGameMetadata(gameMenu!.game)}>Refresh game information</button>
    <button role="menuitem" disabled={!hasSteamAppId(gameMenu.game)} onclick={() => exportGoldbergAchievements(gameMenu!.game)}>Generate Goldberg achievements.json</button>
    <button role="menuitem" onclick={() => clearGameMetadata(gameMenu!.game)}>Clear cached information</button>
    <button role="menuitem" disabled={gameMenu.game.playtimeSeconds === 0 && gameMenu.game.lastPlayed === 0} onclick={() => resetGameActivity(gameMenu!.game)}>Reset playtime and last played</button>
    <button role="menuitem" onclick={() => { gameMenu = null; invoke('open_data_location', { location: 'data' }).catch((error) => status = String(error)); }}>Open Achievement Watcher data folder</button>
    <div class="context-separator"></div>
    <button role="menuitem" disabled={!hasSteamAppId(gameMenu.game)} onclick={() => openGameWebsite(gameMenu!.game, 'steam')}>Steam store</button>
    <button role="menuitem" disabled={!hasSteamAppId(gameMenu.game)} onclick={() => openGameWebsite(gameMenu!.game, 'steamdb')}>SteamDB</button>
    <button role="menuitem" disabled={!hasSteamAppId(gameMenu.game)} onclick={() => openGameWebsite(gameMenu!.game, 'pcgamingwiki')}>PCGamingWiki</button>
    <div class="context-separator"></div>
    <button role="menuitem" class="danger" onclick={() => blacklistGame(gameMenu!.game)}>Add to blacklist</button>
  </div>
{/if}
{#if gameConfig}
  <div class="dialog-overlay" role="presentation">
    <div class="game-config-dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="game-config-title">
      <h2 id="game-config-title">Launch {gameConfig.game.name}</h2>
      <label><span>Executable</span><div><input readonly value={gameConfig.executable} placeholder="Choose an .exe, .bat, or .cmd file" /><button onclick={chooseGameExecutable}>Browse</button></div></label>
      <label><span>Launch arguments</span><input bind:value={gameConfig.arguments} placeholder="Optional" /></label>
      <div class="dialog-actions"><button onclick={() => gameConfig = null}>Cancel</button><button onclick={saveGameConfig} disabled={!gameConfig.executable}>Save</button></div>
    </div>
  </div>
{/if}
<footer><span class:busy={scanning}></span>{status}</footer>
