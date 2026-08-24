<script lang="ts">
  interface Props {
    scanning: boolean;
    settingsActive: boolean;
    onMinimize: () => void;
    onSettings: () => void;
    onMaximize: () => void;
    onClose: () => void;
  }

  let { scanning, settingsActive, onMinimize, onSettings, onMaximize, onClose }: Props = $props();
</script>

<header class="title-bar" data-tauri-drag-region>
  <div class="watcher-state">
    <span class:busy={scanning}></span>
    <span>{scanning ? 'Scanning achievements…' : 'Achievement Watcher is running'}</span>
  </div>
  <nav aria-label="Window controls">
    <button aria-label="Minimize" title="Minimize" onclick={onMinimize}><i class="far fa-window-minimize"></i></button>
    <button aria-label="Settings" title="Settings" class:active={settingsActive} disabled={settingsActive} onclick={onSettings}><i class="fas fa-cog"></i></button>
    <button aria-label="Maximize" title="Maximize" onclick={onMaximize}><i class="far fa-window-maximize"></i></button>
    <button class="close" aria-label="Close" title="Close" onclick={onClose}><i class="fas fa-times"></i></button>
  </nav>
</header>
