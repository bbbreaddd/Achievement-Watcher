<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    gameName: string;
    executable: string;
    launchArguments: string;
    onBrowse: () => void | Promise<void>;
    onSave: () => void | Promise<void>;
    onCancel: () => void;
  }

  let { gameName, executable = $bindable(), launchArguments = $bindable(), onBrowse, onSave, onCancel }: Props = $props();
  let dialog: HTMLElement;

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key !== 'Tab') return;
    const items = Array.from(dialog.querySelectorAll<HTMLElement>('input, button:not(:disabled)'));
    if (!items.length) return;
    const first = items[0];
    const last = items.at(-1)!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  onMount(() => dialog.querySelector<HTMLElement>('input, button')?.focus());
</script>

<div class="dialog-overlay" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) onCancel(); }}>
  <div bind:this={dialog} class="game-config-dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="game-config-title" onkeydown={handleKeydown}>
    <h2 id="game-config-title">Launch {gameName}</h2>
    <label><span>Executable</span><div><input readonly bind:value={executable} placeholder="Choose an .exe, .bat, or .cmd file" /><button onclick={onBrowse}>Browse</button></div></label>
    <label><span>Launch arguments</span><input bind:value={launchArguments} placeholder="Optional" /></label>
    <div class="dialog-actions"><button onclick={onCancel}>Cancel</button><button onclick={onSave} disabled={!executable}>Save</button></div>
  </div>
</div>
