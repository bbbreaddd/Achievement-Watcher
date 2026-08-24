<script lang="ts">
  interface Props {
    busy: boolean;
    message: string;
  }

  let { busy, message }: Props = $props();
</script>

<footer role="status" aria-live="polite" aria-atomic="true" title={message}>
  <span class:busy></span>
  <span class="footer-message">{message}</span>
</footer>
