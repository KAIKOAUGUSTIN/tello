<script lang="ts">
  import { autopilot, selectRect } from '../ai/autopilot.svelte';
  import { labelOf, rectFrom } from '../ai/boxes';
  import { ai } from '../ai/engine.svelte';
  import type { PlayerStats } from '../video/player';

  let { stats }: { stats: PlayerStats | null } = $props();

  // Mesmo enquadramento do canvas (object-fit: contain): o viewBox tem o tamanho do quadro.
  const w = $derived(stats?.width || 960);
  const h = $derived(stats?.height || 720);

  interface Drag {
    x0: number;
    y0: number;
    x1: number;
    y1: number;
    id: number;
  }

  let svg: SVGSVGElement | undefined = $state();
  let drag = $state<Drag | null>(null);
  const dragBox = $derived(drag ? rectFrom(drag.x0, drag.y0, drag.x1, drag.y1) : null);

  /** Ponto da tela em fração do quadro, descontando as faixas pretas. */
  function toFrame(e: PointerEvent): [number, number] {
    const r = svg!.getBoundingClientRect();
    const scale = Math.min(r.width / w, r.height / h);
    const ox = (r.width - w * scale) / 2;
    const oy = (r.height - h * scale) / 2;
    return [(e.clientX - r.left - ox) / (w * scale), (e.clientY - r.top - oy) / (h * scale)];
  }

  function down(e: PointerEvent): void {
    if (!autopilot.selecting) return;
    e.preventDefault();
    svg!.setPointerCapture(e.pointerId);
    const [x, y] = toFrame(e);
    drag = { x0: x, y0: y, x1: x, y1: y, id: e.pointerId };
  }

  function move(e: PointerEvent): void {
    if (!drag || e.pointerId !== drag.id) return;
    [drag.x1, drag.y1] = toFrame(e);
  }

  // Toque (ou arraste curto) escolhe a caixa sob o dedo; arraste maior marca uma área.
  function up(e: PointerEvent): void {
    if (!drag || e.pointerId !== drag.id) return;
    const r = rectFrom(drag.x0, drag.y0, drag.x1, drag.y1);
    drag = null;
    if (autopilot.selecting) selectRect(r);
  }

  const font = $derived(Math.round(w * 0.02));
  const labelY = (y: number) => Math.max(font + 4, y * h - 6);
</script>

<svg
  class="ai"
  class:selecting={autopilot.selecting}
  bind:this={svg}
  viewBox="0 0 {w} {h}"
  preserveAspectRatio="xMidYMid meet"
  role="application"
  aria-label="Escolher o que seguir"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={() => (drag = null)}
>
  <!-- Motor simulado: o que estaria na frente da câmera. -->
  {#each ai.mockObjects as o (o.id)}
    {#if o.shape === 'balloon'}
      <ellipse
        class="mock"
        cx={(o.box.x + o.box.w / 2) * w}
        cy={(o.box.y + o.box.h / 2) * h}
        rx={(o.box.w / 2) * w}
        ry={(o.box.h / 2) * h}
      />
    {:else}
      <rect class="mock" x={o.box.x * w} y={o.box.y * h} width={o.box.w * w} height={o.box.h * h} rx={o.shape === 'person' ? w * 0.02 : w * 0.01} />
    {/if}
  {/each}

  {#if autopilot.selecting}
    {#each ai.detections as d, i (i)}
      <g class="candidate">
        <rect x={d.x * w} y={d.y * h} width={d.w * w} height={d.h * h} />
        <text x={d.x * w + 4} y={labelY(d.y)} font-size={font}>{labelOf(d.category)}</text>
      </g>
    {/each}
  {/if}

  {#if dragBox}
    <rect class="drag" x={dragBox.x * w} y={dragBox.y * h} width={dragBox.w * w} height={dragBox.h * h} />
  {/if}

  {#if autopilot.follow && autopilot.box}
    {@const b = autopilot.box}
    <g class="target" class:lost={autopilot.lost}>
      <rect x={b.x * w} y={b.y * h} width={b.w * w} height={b.h * h} />
      <text x={b.x * w + 4} y={labelY(b.y)} font-size={font}>{autopilot.label}</text>
    </g>
  {/if}
</svg>

<style>
  .ai {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }

  /* Escolhendo: a tela inteira recebe o toque, por cima dos sticks. */
  .ai.selecting {
    z-index: 5;
    pointer-events: auto;
    touch-action: none;
    cursor: crosshair;
  }

  rect,
  ellipse {
    fill: none;
    vector-effect: non-scaling-stroke;
  }

  text {
    font-family: var(--font-display);
    font-weight: 700;
    paint-order: stroke;
    stroke: rgb(0 0 0 / 0.65);
    stroke-width: 3px;
  }

  .mock {
    fill: rgb(255 255 255 / 0.18);
    stroke: rgb(255 255 255 / 0.45);
    stroke-width: 1.5px;
  }

  .candidate rect {
    fill: rgb(148 163 184 / 0.12);
    stroke: rgb(203 213 225 / 0.85);
    stroke-width: 2px;
  }

  .candidate text {
    fill: rgb(226 232 240);
  }

  .drag {
    fill: rgb(45 212 238 / 0.12);
    stroke: var(--accent);
    stroke-width: 2px;
    stroke-dasharray: 6 4;
  }

  .target rect {
    stroke: var(--accent);
    stroke-width: 3px;
  }

  .target text {
    fill: var(--accent);
  }

  .target.lost rect {
    stroke: var(--warn);
    stroke-dasharray: 8 6;
  }

  .target.lost text {
    fill: var(--warn);
  }
</style>
