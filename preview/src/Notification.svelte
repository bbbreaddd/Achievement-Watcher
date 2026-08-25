<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import type { NotificationEvent, NotificationPresentationSettings, NotificationRenderRequest } from './types';
  import NotificationCard from './components/NotificationCard.svelte';
  import steamSound from '../../app/media/Steam Deck.wav';
  import windowsSound from '../../app/media/Windows 10.wav';
  import windows11Sound from '../../app/media/Windows 11.wav';
  import playstationSound from '../../app/media/Playstation.wav';
  import playstation5Sound from '../../app/media/Playstation5.wav';
  import playstationPlatinumSound from '../../app/media/Playstation5 Platinum.wav';
  import gogSound from '../../app/media/GOG Galaxy.wav';
  import androidSound from '../../app/media/Android 9 Notification Popcorn.wav';

  let event: NotificationEvent | null = null;
  let active = false;
  let closeTimer: number | undefined;
  let duration = 4_000;
  let showDescription = true;
  let preset = 'steam';
  let presetConfig = { width: 382, height: 106, durationMs: 4_000 };
  let sound = 'steam_deck';
  let customSoundPath: string | undefined;
  let scalePercent = 100;
  let presetFallback = false;
  let resolvePresetReady: (() => void) | undefined;

  function usesOriginalPreset() {
    return Boolean(event && event.kind === 'unlock' && !event.eventKey.startsWith('playtime') && preset !== 'default' && preset !== 'original' && preset !== 'steam');
  }

  function presetLoaded() {
    resolvePresetReady?.();
    resolvePresetReady = undefined;
  }

  function applyPresentation(settings: NotificationPresentationSettings, config: NotificationRenderRequest['presetConfig']) {
    showDescription = settings.showDescription;
    preset = settings.preset;
    presetConfig = config;
    duration = Math.round(config.durationMs * settings.durationPercent / 100);
    sound = settings.sound;
    customSoundPath = settings.customSoundPath;
    scalePercent = settings.scalePercent;
  }

  function clearCloseTimer() {
    window.clearTimeout(closeTimer);
    closeTimer = undefined;
  }

  async function showEvent(payload: NotificationRenderRequest) {
    clearCloseTimer();
    applyPresentation(payload.presentation, payload.presetConfig);
    event = payload.event;
    presetFallback = false;
    active = true;
    const sounds: Record<string, string> = { steam_deck: steamSound, windows: windowsSound, windows_11: windows11Sound, playstation: playstationSound, playstation_5: playstation5Sound, playstation_platinum: playstationPlatinumSound, gog: gogSound, android: androidSound };
    const soundUrl = sound === 'custom' && customSoundPath
      ? await invoke<string>('read_notification_audio', { path: customSoundPath }).catch(() => undefined)
      : sounds[sound];
    if (soundUrl) void new Audio(soundUrl).play().catch(() => undefined);
    const presetReady = usesOriginalPreset()
      ? new Promise<void>((resolve) => { resolvePresetReady = resolve; })
      : undefined;
    await new Promise(requestAnimationFrame);
    if (presetReady) {
      await Promise.race([
        presetReady,
        new Promise<void>((resolve) => window.setTimeout(resolve, 1_500)),
      ]);
      if (resolvePresetReady) {
        resolvePresetReady = undefined;
        presetFallback = true;
        await new Promise(requestAnimationFrame);
      }
    }
    await invoke('acknowledge_notification', { eventId: payload.event.id });
    closeTimer = window.setTimeout(async () => {
      active = false;
      await new Promise((resolve) => window.setTimeout(resolve, 250));
      await invoke('close_notification');
    }, duration);
  }

  async function close() {
    clearCloseTimer();
    active = false;
    await new Promise((resolve) => window.setTimeout(resolve, 200));
    await invoke('close_notification');
  }

  async function openGame() {
    clearCloseTimer();
    active = false;
    await invoke('open_notification_game');
  }

  async function reportError(error: unknown) {
    await invoke('report_notification_error', { message: String(error) }).catch(() => undefined);
  }

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        unlisten = await listen<NotificationRenderRequest>('notification-request', ({ payload }) => {
          void showEvent(payload).catch(reportError);
        });
        if (disposed) return unlisten();
        const pending = await invoke<NotificationRenderRequest | null>('current_notification');
        if (pending && !disposed) await showEvent(pending);
      } catch (error) {
        await reportError(error);
      }
    })();
    const receivePresetMessage = (message: MessageEvent) => {
      if (message.origin === location.origin && message.data?.type === 'achievement-watcher-preset-open') void openGame();
    };
    window.addEventListener('message', receivePresetMessage);
    return () => {
      disposed = true;
      unlisten?.();
      clearCloseTimer();
      window.removeEventListener('message', receivePresetMessage);
    };
  });
</script>

<svelte:window onkeydown={(keyboardEvent) => { if (keyboardEvent.key === 'Escape') void close().catch(reportError); }} />
{#if event}<NotificationCard {event} {showDescription} {preset} {duration} {scalePercent} {presetConfig} {active} forceFallback={presetFallback} onopen={() => { void openGame().catch(reportError); }} onclose={() => { void close().catch(reportError); }} onready={presetLoaded} />{/if}
