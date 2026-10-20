<script lang="ts">
  import { icons, type IconName } from '../lib/icons';

  let { name, size = 18, stroke = 2, class: klass = '' }: { name: IconName; size?: number; stroke?: number; class?: string } =
    $props();
</script>

<svg
  class={klass}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {@html icons[name]}
</svg>
