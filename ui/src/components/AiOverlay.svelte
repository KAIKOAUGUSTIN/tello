<script lang="ts">
  import { autopilot } from '../ai/autopilot.svelte';
  import { ai } from '../ai/engine.svelte';
  import { BONES, JOINTS, MIN_VISIBILITY } from '../ai/pose';
  import type { PlayerStats } from '../video/player';

  let { stats }: { stats: PlayerStats | null } = $props();

  // Mesmo enquadramento do canvas (object-fit: contain): o viewBox tem o tamanho do quadro.
  const w = $derived(stats?.width || 960);
  const h = $derived(stats?.height || 720);
</script>

{#if ai.poses.length}
  <svg class="ai" viewBox="0 0 {w} {h}" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
    {#each ai.poses as pose, i (i)}
      <!-- Com o seguir ligado, a pessoa travada fica em destaque e as outras apagadas. -->
      <g
        class="pose"
        class:target={autopilot.target?.pose === pose}
        class:other={autopilot.follow && autopilot.target?.pose !== pose}
      >
        {#each BONES as [a, b] (`${a}-${b}`)}
          {#if pose[a * 3 + 2] >= MIN_VISIBILITY && pose[b * 3 + 2] >= MIN_VISIBILITY}
            <line x1={pose[a * 3] * w} y1={pose[a * 3 + 1] * h} x2={pose[b * 3] * w} y2={pose[b * 3 + 1] * h} />
          {/if}
        {/each}
        {#each JOINTS as j (j)}
          {#if pose[j * 3 + 2] >= MIN_VISIBILITY}
            <circle cx={pose[j * 3] * w} cy={pose[j * 3 + 1] * h} r={w * 0.006} />
          {/if}
        {/each}
      </g>
    {/each}
  </svg>
{/if}

<style>
  .ai {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }

  line {
    stroke: var(--accent);
    stroke-width: 3px;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
    opacity: 0.85;
  }

  circle {
    fill: var(--bg);
    stroke: var(--accent);
    stroke-width: 2px;
    vector-effect: non-scaling-stroke;
  }

  .target line {
    stroke-width: 4px;
    opacity: 1;
  }

  .other line,
  .other circle {
    stroke: var(--hud-dim);
    opacity: 0.5;
  }
</style>
