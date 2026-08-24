<script lang="ts">
  interface Props {
    scanning: boolean;
    settingsActive: boolean;
    maximized: boolean;
    onMinimize: () => void;
    onSettings: () => void;
    onMaximize: () => void;
    onClose: () => void;
  }

  let { scanning, settingsActive, maximized, onMinimize, onSettings, onMaximize, onClose }: Props = $props();
</script>

<header class="title-bar" data-tauri-drag-region>
  <div class="watcher-state">
    <span class:busy={scanning}></span>
    <span>{scanning ? 'Scanning achievements…' : 'Achievement Watcher is running'}</span>
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
