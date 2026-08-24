import type { GameSummary } from './types';

export function completionPercent(game: GameSummary): number {
  if (game.total <= 0) return 0;
  return Math.min(100, Math.max(0, (game.unlocked / game.total) * 100));
}

export function sourceLabel(source?: string): string {
  switch (source) {
    case 'steam': return 'Steam client';
    case 'steam_emulator': return 'Steam emulator save';
    case 'green_luma': return 'GreenLuma save';
    case 'rpcs3': return 'RPCS3 trophy data';
    case 'epic': return 'Epic emulator save';
    case 'gog': return 'GOG emulator save';
    case 'luma_play': return 'LumaPlay';
    case 'watchdog_cache': return 'Achievement Watcher cache';
    default: return 'Cached Steam metadata';
  }
}

export function sourceDescription(source?: string): string {
  switch (source) {
    case 'steam': return 'Achievement progress read from the Steam client';
    case 'steam_emulator': return 'Achievement progress read from a local Steam emulator save file';
    case 'green_luma': return 'Achievement progress read from a local GreenLuma save file';
    case 'rpcs3': return 'Trophy progress read from local RPCS3 data';
    case 'epic': return 'Achievement progress read from a Nemirtingas Epic emulator save';
    case 'gog': return 'Achievement progress read from a Nemirtingas Galaxy emulator save';
    case 'luma_play': return 'Achievement progress read from the local LumaPlay registry';
    case 'watchdog_cache': return 'Achievement progress imported from the original Achievement Watcher cache';
    default: return 'Game and achievement information is cached; no local progress source was found';
  }
}
