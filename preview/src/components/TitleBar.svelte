<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  interface Props {
    activity: string | null;
    settingsActive: boolean;
    maximized: boolean;
    onMinimize: () => void;
    onSettings: () => void;
    onMaximize: () => void;
    onClose: () => void;
  }

  let { activity, settingsActive, maximized, onMinimize, onSettings, onMaximize, onClose }: Props = $props();

  function touchDrag(node: HTMLElement) {
    const start = (event: PointerEvent) => {
      if (event.isPrimary && event.pointerType !== 'mouse' && !(event.target as HTMLElement).closest('button')) {
        void getCurrentWindow().startDragging();
      }
    };
    node.addEventListener('pointerdown', start);
    return {
      destroy: () => node.removeEventListener('pointerdown', start),
    };
  }
</script>

<header class="title-bar" data-tauri-drag-region use:touchDrag>
  <div class="watcher-state">
    <span class:busy={Boolean(activity)}></span>
    <span>{activity ?? 'Achievement Watcher is running'}</span>
  </div>
  <div class="title-actions">
    <button class="settings" aria-label="Settings" title="Settings" class:active={settingsActive} disabled={settingsActive} onclick={onSettings}><i class="fas fa-cog"></i></button>
    <nav aria-label="Window controls">
      <button aria-label="Minimize" title="Minimize" onclick={onMinimize}><i class="far fa-window-minimize"></i></button>
      <button aria-label={maximized ? 'Restore' : 'Maximize'} title={maximized ? 'Restore' : 'Maximize'} onclick={onMaximize}><i class={maximized ? 'far fa-window-restore' : 'far fa-window-maximize'}></i></button>
      <button class="close" aria-label="Close" title="Close" onclick={onClose}><i class="fas fa-times"></i></button>
    </nav>
  </div>
</header>
