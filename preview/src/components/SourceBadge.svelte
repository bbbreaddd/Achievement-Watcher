<script lang="ts">
  import { sourceDescription } from '../library';
  import type { SourceKind } from '../types';
  import steamIcon from '../../../app/Source/steam.svg';
  import playstationIcon from '../../../app/Source/playstation.svg';
  import epicIcon from '../../../app/Source/epic.svg';
  import gogIcon from '../../../app/Source/gog.svg';

  interface Props {
    source?: SourceKind;
    description?: string;
    large?: boolean;
    origin?: boolean;
  }

  let { source, description, large = false, origin = false }: Props = $props();

  function icon() {
    if (source === 'rpcs3') return playstationIcon;
    if (source === 'steam' || source === 'steam_emulator' || source === 'green_luma') return steamIcon;
    if (source === 'epic') return epicIcon;
    if (source === 'gog') return gogIcon;
    return null;
  }

  function mark() {
    if (source === 'green_luma') return 'GL';
    if (!source) return 'DB';
    return source.slice(0, 2).toUpperCase();
  }
</script>

<i class="source-badge" class:large class:achievement-origin={origin} title={description ?? sourceDescription(source)}>
  {#if icon()}<img src={icon()!} alt="" />{:else}{mark()}{/if}
</i>
