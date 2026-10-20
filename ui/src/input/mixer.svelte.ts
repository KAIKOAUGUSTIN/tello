// Combina as fontes de entrada (mouse, teclado, controle) em um único comando.
// A fonte que se mexeu por último assume; sem foco na janela, tudo vai a zero.

import { NEUTRAL, type Sticks } from '../lib/types';
import { isNeutral } from './mapping';

export type SourceId = 'mouse' | 'keyboard' | 'gamepad';

interface SourceState {
  sticks: Sticks;
  activeAt: number;
}

export const input = $state({
  source: null as SourceId | null,
  sticks: NEUTRAL as Sticks,
  focused: true,
  gamepad: null as string | null,
});

const sources = new Map<SourceId, SourceState>();

export function report(id: SourceId, sticks: Sticks): void {
  const prev = sources.get(id);
  const active = !isNeutral(sticks);
  sources.set(id, { sticks, activeAt: active ? performance.now() : (prev?.activeAt ?? 0) });
  if (active) input.source = id;
}

function mix(): Sticks {
  let best: SourceState | null = null;
  for (const s of sources.values()) {
    if (!isNeutral(s.sticks) && (!best || s.activeAt > best.activeAt)) best = s;
  }
  return best ? best.sticks : NEUTRAL;
}

const SEND_INTERVAL_MS = 33;

/** Envia os sticks a ~30 Hz enquanto `enabled()` for verdadeiro. */
export function startMixer(send: (s: Sticks) => void, enabled: () => boolean): () => void {
  const onBlur = () => (input.focused = false);
  const onFocus = () => (input.focused = true);
  const onVisibility = () => (input.focused = document.visibilityState === 'visible' && document.hasFocus());
  window.addEventListener('blur', onBlur);
  window.addEventListener('focus', onFocus);
  document.addEventListener('visibilitychange', onVisibility);
  input.focused = document.hasFocus();

  const timer = window.setInterval(() => {
    const s = input.focused ? mix() : NEUTRAL;
    const prev = input.sticks;
    if (s.roll !== prev.roll || s.pitch !== prev.pitch || s.throttle !== prev.throttle || s.yaw !== prev.yaw) {
      input.sticks = s;
    }
    // Envia mesmo sem mudança: o núcleo zera os sticks se as atualizações pararem.
    if (enabled()) send(s);
  }, SEND_INTERVAL_MS);

  return () => {
    clearInterval(timer);
    window.removeEventListener('blur', onBlur);
    window.removeEventListener('focus', onFocus);
    document.removeEventListener('visibilitychange', onVisibility);
  };
}
