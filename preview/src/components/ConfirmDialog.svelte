<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    title: string;
    message: string;
    confirmLabel: string;
    busy?: boolean;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { title, message, confirmLabel, busy = false, danger = true, onConfirm, onCancel }: Props = $props();
  let dialog: HTMLElement;
  let cancelButton: HTMLButtonElement;

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key !== 'Tab') return;
    const controls = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled)'));
    if (!controls.length) return;
    const first = controls[0];
    const last = controls.at(-1)!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  onMount(() => cancelButton.focus());
</script>

<div class="dialog-overlay" role="presentation" onclick={(event) => { if (event.target === event.currentTarget && !busy) onCancel(); }}>
  <div bind:this={dialog} class="confirm-dialog" role="alertdialog" tabindex="-1" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message" onkeydown={handleKeydown}>
    <h2 id="confirm-title">{title}</h2>
    <p id="confirm-message">{message}</p>
    <div class="dialog-actions">
      <button bind:this={cancelButton} disabled={busy} onclick={onCancel}>Cancel</button>
      <button class:danger-action={danger} class:primary={!danger} disabled={busy} onclick={onConfirm}>{busy ? 'Working…' : confirmLabel}</button>
    </div>
  </div>
</div>
