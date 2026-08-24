<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import type { AppSettings, NotificationEvent } from './types';
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
  let sound = 'steam_deck';
  let customSoundPath: string | undefined;
  let scalePercent = 100;
  let presetFrame: HTMLIFrameElement | undefined;
  let presetFailed = false;
  let resolvePresetReady: (() => void) | undefined;

  function imageUrl(value: string) {
    return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') ? convertFileSrc(value) : value;
  }

  function notificationHeading() {
    if (event?.eventKey.startsWith('playtime')) return 'Playtime tracking';
    if (event?.kind === 'progress') return 'Achievement progress';
    return 'Achievement unlocked';
  }

  function presetSize() {
    const sizes: Record<string, [number, number]> = {
      default: [420, 110], original: [420, 110], ps4: [400, 200], ps5: [400, 150],
      ps5_enhanced: [450, 150], xbox_one: [600, 160], xbox_360: [600, 150],
      raposo: [400, 150], smooth_pop: [400, 150], xqjan: [450, 150], steam: [382, 106],
    };
    return sizes[preset] ?? sizes.steam;
  }

  function presetDuration(value = preset) {
    const durations: Record<string, number> = {
      default: 6_000, original: 6_000, ps4: 5_000, ps5: 4_000,
      ps5_enhanced: 4_000, xbox_one: 10_000, xbox_360: 5_000,
      raposo: 6_000, smooth_pop: 8_000, xqjan: 10_000, steam: 4_000,
    };
    return durations[value] ?? 4_000;
  }

  function presetDocument() {
    const documents: Record<string, string> = {
      default: 'Default', original: 'Default', ps4: 'PS4', ps5: 'PS5',
      ps5_enhanced: 'PS5enhanced', xbox_one: 'Xbox One', xbox_360: 'Xbox360',
      raposo: 'Raposo', smooth_pop: 'SmoothPop', xqjan: 'xqjan',
    };
    return documents[preset] ? `/${encodeURIComponent(documents[preset])}/index.html` : '';
  }

  function usesOriginalPreset() {
    return Boolean(!presetFailed && event && event.kind === 'unlock' && !event.eventKey.startsWith('playtime') && preset !== 'default' && preset !== 'original' && presetDocument());
  }

  function sendPreset() {
    if (!event || !presetFrame?.contentWindow) return;
    presetFrame.contentWindow.postMessage({
      type: 'achievement-watcher-notification',
      displayName: event.observation.displayName ?? event.observation.achievementId,
      description: showDescription ? (event.observation.description ?? '') : '',
      iconPath: event.observation.icon ? imageUrl(event.observation.icon) : '',
      duration,
    }, location.origin);
  }

  function presetLoaded() {
    sendPreset();
    resolvePresetReady?.();
    resolvePresetReady = undefined;
  }

  function presetLoadFailed() {
    presetFailed = true;
    resolvePresetReady?.();
    resolvePresetReady = undefined;
  }

  async function loadNotificationSettings() {
    const settings = await invoke<AppSettings>('load_settings');
    showDescription = settings.notificationShowDescription;
    preset = settings.notificationPreset;
    duration = Math.round(presetDuration(preset) * settings.notificationDurationPercent / 100);
    sound = settings.notificationSound;
    customSoundPath = settings.notificationCustomSoundPath;
    scalePercent = settings.notificationScalePercent;
  }

  async function showEvent(payload: NotificationEvent) {
    window.clearTimeout(closeTimer);
    await loadNotificationSettings().catch(() => undefined);
    event = payload;
    presetFailed = false;
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
        presetFailed = true;
        await new Promise(requestAnimationFrame);
      }
    }
    await invoke('acknowledge_notification', { eventId: payload.id });
    closeTimer = window.setTimeout(async () => {
      active = false;
      await new Promise((resolve) => window.setTimeout(resolve, 250));
      await invoke('close_notification');
    }, duration);
  }

  async function close() {
    active = false;
    await invoke('close_notification');
  }

  async function openGame() {
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
        unlisten = await listen<NotificationEvent>('notification-request', ({ payload }) => {
          void showEvent(payload).catch(reportError);
        });
        if (disposed) return unlisten();
        try {
          await loadNotificationSettings();
        } catch {
          duration = 4_000;
        }
        const pending = await invoke<NotificationEvent | null>('current_notification');
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
      window.clearTimeout(closeTimer);
      window.removeEventListener('message', receivePresetMessage);
    };
  });
</script>

<div class="notification-stage" style={`width:${presetSize()[0]}px;height:${presetSize()[1]}px;zoom:${scalePercent / 100}`}>{#if usesOriginalPreset()}<iframe bind:this={presetFrame} class="original-preset-frame" src={presetDocument()} title="Achievement notification" onload={presetLoaded} onerror={presetLoadFailed}></iframe><button class="notification-close preset-close" aria-label="Close notification" onclick={close}>×</button>{:else}<div class:active class:original={preset === 'original' || preset === 'default'} class:ps4={preset === 'ps4'} class:ps5={preset === 'ps5' || preset === 'ps5_enhanced'} class:ps5enhanced={preset === 'ps5_enhanced'} class:xbox={preset === 'xbox_one' || preset === 'xbox_360'} class:xbox360={preset === 'xbox_360'} class:smooth={preset === 'smooth_pop'} class:raposo={preset === 'raposo'} class:xqjan={preset === 'xqjan'} class="notification-shell" role="button" tabindex="0" aria-label="Open achievement" onclick={openGame} onkeydown={(keyboardEvent) => { if (keyboardEvent.key === 'Enter' || keyboardEvent.key === ' ') void openGame(); }}>
  <button class="notification-close" aria-label="Close notification" onclick={(mouseEvent) => { mouseEvent.stopPropagation(); void close(); }}>×</button>
  <div class="achievement-mark"><span>◆</span>{#if event?.observation.icon}<img src={imageUrl(event.observation.icon)} alt="" onerror={(imageEvent) => imageEvent.currentTarget.remove()} />{/if}</div>
  <div class="notification-copy">
    <span>{notificationHeading()}</span>
    <strong>{event?.observation.displayName ?? event?.observation.achievementId ?? ''}</strong>
    {#if event?.kind === 'progress' && event.observation.maxProgress > 0}<div class="notification-progress"><i style={`width:${Math.min(100, event.observation.currentProgress / event.observation.maxProgress * 100)}%`}></i><span>{event.observation.currentProgress} / {event.observation.maxProgress}</span></div>{/if}
    {#if showDescription}<p>{event?.observation.description ?? ''}</p>{/if}
  </div>
</div>{/if}</div>
