<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import type { NotificationEvent, NotificationRenderRequest } from '../types';

  export let event: NotificationEvent;
  export let showDescription = true;
  export let preset = 'steam';
  export let duration = 4_000;
  export let scalePercent = 100;
  export let presetConfig: NotificationRenderRequest['presetConfig'];
  export let active = true;
  export let controls = true;
  export let replay = false;
  export let forceFallback = false;
  export let onopen: () => void = () => undefined;
  export let onclose: () => void = () => undefined;
  export let onready: () => void = () => undefined;

  let presetFrame: HTMLIFrameElement | undefined;
  let presetFailed = false;
  let replayTimer: ReturnType<typeof setTimeout> | undefined;

  function imageUrl(value: string) {
    return /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith('\\\\') ? convertFileSrc(value) : value;
  }

  function heading() {
    if (event.eventKey.startsWith('playtime')) return 'Playtime tracking';
    if (event.kind === 'progress') return 'Achievement progress';
    return 'Achievement unlocked';
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
    return !forceFallback && !presetFailed && event.kind === 'unlock' && !event.eventKey.startsWith('playtime')
      && preset !== 'default' && preset !== 'original' && Boolean(presetDocument());
  }

  function sendPreset() {
    if (!presetFrame?.contentWindow) return;
    presetFrame.contentWindow.postMessage({
      type: 'achievement-watcher-notification',
      displayName: event.observation.displayName ?? event.observation.achievementId,
      description: showDescription ? (event.observation.description ?? '') : '',
      iconPath: event.observation.icon ? imageUrl(event.observation.icon) : '',
      duration,
    }, location.origin);
  }

  function scheduleReplay() {
    clearTimeout(replayTimer);
    if (!replay || !usesOriginalPreset()) return;
    replayTimer = setTimeout(() => {
      const container = presetFrame?.contentDocument?.querySelector<HTMLElement>('.ach');
      container?.classList.remove('active');
      if (container) void container.offsetWidth;
      sendPreset();
      scheduleReplay();
    }, duration + 750);
  }

  function presetLoaded() {
    sendPreset();
    scheduleReplay();
    onready();
  }

  function presetLoadFailed() {
    clearTimeout(replayTimer);
    presetFailed = true;
    onready();
  }

  $: {
    preset;
    presetFailed = false;
  }
  $: if (presetFrame && event) {
    showDescription;
    duration;
    queueMicrotask(() => {
      sendPreset();
      scheduleReplay();
    });
  }

  onDestroy(() => clearTimeout(replayTimer));
</script>

<div class="notification-stage" style={`width:${presetConfig.width}px;height:${presetConfig.height}px;zoom:${scalePercent / 100}`}>
  {#if usesOriginalPreset()}
    <iframe bind:this={presetFrame} class="original-preset-frame" src={presetDocument()} title="Achievement notification" onload={presetLoaded} onerror={presetLoadFailed}></iframe>
    {#if controls}<button class="notification-close preset-close" aria-label="Close notification" onclick={onclose}>×</button>{/if}
  {:else}
    <div class:active class:original={preset === 'original' || preset === 'default'} class:ps4={preset === 'ps4'} class:ps5={preset === 'ps5' || preset === 'ps5_enhanced'} class:ps5enhanced={preset === 'ps5_enhanced'} class:xbox={preset === 'xbox_one' || preset === 'xbox_360'} class:xbox360={preset === 'xbox_360'} class:smooth={preset === 'smooth_pop'} class:raposo={preset === 'raposo'} class:xqjan={preset === 'xqjan'} class="notification-shell">
      {#if controls}<button class="notification-open" aria-label="Open achievement" onclick={onopen}></button><button class="notification-close" aria-label="Close notification" onclick={(mouseEvent) => { mouseEvent.stopPropagation(); onclose(); }}>×</button>{/if}
      <div class="achievement-mark"><span>◆</span>{#if event.observation.icon}<img src={imageUrl(event.observation.icon)} alt="" onerror={(imageEvent) => imageEvent.currentTarget.remove()} />{/if}</div>
      <div class="notification-copy">
        <span>{heading()}</span>
        <strong>{event.observation.displayName ?? event.observation.achievementId}</strong>
        {#if event.kind === 'progress' && event.observation.maxProgress > 0}<div class="notification-progress"><i style={`width:${Math.min(100, event.observation.currentProgress / event.observation.maxProgress * 100)}%`}></i><span>{event.observation.currentProgress} / {event.observation.maxProgress}</span></div>{/if}
        {#if showDescription}<p>{event.observation.description ?? ''}</p>{/if}
      </div>
    </div>
  {/if}
</div>
