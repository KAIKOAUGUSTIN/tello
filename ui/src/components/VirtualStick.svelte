<script lang="ts">
  import type { Axis2 } from '../input/mapping';

  let {
    value = $bindable({ x: 0, y: 0 }),
    label,
    hint,
    size = 150,
  }: { value?: Axis2; label: string; hint: string; size?: number } = $props();

  let base: HTMLDivElement;
  let pointer: number | null = $state(null);
  const knob = $derived(size * 0.38);
  const travel = $derived((size - knob) / 2);

  function update(e: PointerEvent) {
    const r = base.getBoundingClientRect();
    let x = (e.clientX - (r.left + r.width / 2)) / travel;
    let y = -(e.clientY - (r.top + r.height / 2)) / travel;
    const len = Math.hypot(x, y);
    if (len > 1) {
      x /= len;
      y /= len;
    }
    value = { x, y };
  }

  function down(e: PointerEvent) {
    if (pointer !== null) return;
    pointer = e.pointerId;
    base.setPointerCapture(e.pointerId);
    update(e);
  }

  function move(e: PointerEvent) {
    if (e.pointerId === pointer) update(e);
  }

  function up(e: PointerEvent) {
    if (e.pointerId !== pointer) return;
    pointer = null;
    value = { x: 0, y: 0 };
  }
</script>

<div class="stick">
  <div
    bind:this={base}
    class="base"
    class:active={pointer !== null}
    style="width: {size}px; height: {size}px"
    role="slider"
    aria-label={label}
    aria-valuenow={Math.round(value.y * 100)}
    tabindex="-1"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onlostpointercapture={up}
  >
    <span class="cross h"></span>
    <span class="cross v"></span>
    <span
      class="knob"
      style="width: {knob}px; height: {knob}px; transform: translate({value.x * travel}px, {-value.y * travel}px)"
    ></span>
  </div>
  <span class="caption">{hint}</span>
</div>

<style>
  .stick {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: auto;
  }

  .base {
    position: relative;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 1px solid rgb(255 255 255 / 0.18);
    background: radial-gradient(circle, rgb(10 14 20 / 0.25), rgb(10 14 20 / 0.55));
    backdrop-filter: blur(6px);
    touch-action: none;
    cursor: grab;
    transition: border-color 0.15s;
  }

  .base.active {
    border-color: var(--accent);
    cursor: grabbing;
  }

  .cross {
    position: absolute;
    background: rgb(255 255 255 / 0.1);
  }

  .cross.h {
    width: 70%;
    height: 1px;
  }

  .cross.v {
    width: 1px;
    height: 70%;
  }

  .knob {
    position: absolute;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 30%, #e6fbff, var(--accent) 60%, var(--accent-strong));
    box-shadow:
      0 4px 14px rgb(0 0 0 / 0.45),
      0 0 18px rgb(45 212 238 / 0.35);
  }

  .base:not(.active) .knob {
    transition: transform 0.12s ease-out;
  }

  .caption {
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--glass);
    color: var(--hud-dim);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
</style>
