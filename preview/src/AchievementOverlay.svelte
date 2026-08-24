<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import type { AchievementObservation, AppSettings, GameSummary } from './types';

  let game: GameSummary | null = null;
  let achievements: AchievementObservation[] = [];
  let message = 'Loading achievements…';
  let sort: 'status' | 'name' = 'status';
  let direction = 1;
  let scale = 100;

  function imageUrl(value: string) {
    return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') ? convertFileSrc(value) : value;
  }
  const window = getCurrentWindow();

  function sortedRows() {
    return [...achievements].sort((left, right) => {
      const result = sort === 'status'
        ? Number(right.achieved) - Number(left.achieved)
        : (left.displayName ?? left.achievementId).localeCompare(right.displayName ?? right.achievementId);
      return result * direction;
    });
  }

  function cycle(next: 'status' | 'name') {
    if (sort === next) direction *= -1;
    else { sort = next; direction = 1; }
  }

  async function load() {
    const gameId = await invoke<string>('current_overlay_game_id');
    const settings = await invoke<AppSettings>('load_settings');
    scale = settings.achievementOverlayScalePercent;
    const games = await invoke<GameSummary[]>('list_games');
    game = games.find((item) => item.gameId === gameId && item.sourceId === 'merged')
      ?? games.find((item) => item.gameId === gameId)
      ?? null;
    if (!game) throw new Error(`No achievement data is available for game ${gameId}`);
    achievements = await invoke<AchievementObservation[]>('list_achievements', { sourceId: game.sourceId, gameId });
    message = achievements.length ? '' : 'No achievements found for this game';
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        await load();
        await window.show();
        unlisten = await listen('library-changed', () => { void load(); });
      } catch (error) {
        message = String(error);
        await window.show();
      }
    })();
    return () => unlisten?.();
  });
</script>

<div class="overlay-panel" style={`zoom:${scale / 100}`}>
  <header data-tauri-drag-region>
    <strong>{game?.name ?? 'Achievements Overlay'}</strong>
    {#if game}<span>{game.unlocked} / {game.total}</span>{/if}
    <button title="Close overlay" aria-label="Close overlay" onclick={() => invoke('close_achievement_overlay')}>×</button>
  </header>
  {#if message}<div class="overlay-message">{message}</div>{:else}
  <div class="overlay-head"><span>Icon</span><button onclick={() => cycle('name')}>Achievement {sort === 'name' ? (direction > 0 ? '↑' : '↓') : ''}</button><button onclick={() => cycle('status')}>Status {sort === 'status' ? (direction > 0 ? '↑' : '↓') : ''}</button></div>
  <div class="overlay-rows">{#each sortedRows() as achievement}<article class:unlocked={achievement.achieved}><div class="achievement-icon"><span><i class="fas fa-lock"></i></span>{#if achievement.icon}<img src={imageUrl(achievement.icon)} alt="" onerror={(event) => event.currentTarget.remove()} />{/if}</div><div><strong>{achievement.hidden && !achievement.achieved ? 'Hidden achievement' : (achievement.displayName ?? achievement.achievementId)}</strong><p>{achievement.hidden && !achievement.achieved ? 'Details will be revealed once unlocked' : (achievement.description ?? '')}</p>{#if !achievement.achieved && achievement.maxProgress > 0}<small>Progress: {achievement.currentProgress} / {achievement.maxProgress}</small>{/if}</div><div class="overlay-state"><b>{achievement.achieved ? 'Unlocked' : 'Locked'}</b>{#if achievement.achieved && achievement.unlockTime > 0}<time>{new Date(achievement.unlockTime * 1000).toLocaleString()}</time>{/if}</div></article>{/each}</div>
  {/if}
</div>
