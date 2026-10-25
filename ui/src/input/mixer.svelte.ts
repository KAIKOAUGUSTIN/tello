// Combina as fontes de entrada (mouse, teclado, controle e IA) em um único comando.
// A fonte manual que se mexeu por último assume; a IA só comanda com todas as
// manuais no centro. Sem foco na janela, tudo vai a zero.

import { NEUTRAL, type Sticks } from '../lib/types';
import { isNeutral } from './mapping';
import { pick, type SourceId, type SourceState } from './mix';

export type { SourceId };

export const input = $state({
  source: null as SourceId | null,
  sticks: NEUTRAL as Sticks,
  focused: true,
  gamepad: null as string | null,
});

const sources = new Map<SourceId, SourceState>();
const manualListeners = new Set<(id: SourceId) => void>();

export function report(id: SourceId, sticks: Sticks): void {
  const now = performance.now();
  const prev = sources.get(id);
  const active = !isNeutral(sticks);
  sources.set(id, { sticks, activeAt: active ? now : (prev?.activeAt ?? 0), at: now });
  if (id === 'ai') {
    if (!active && input.source === 'ai') input.source = null;
    return;
  }
  if (active) {
    input.source = id;
    manualListeners.forEach((cb) => cb(id));
  }
}

/** Avisa quando uma fonte manual sai do centro (a IA larga o controle). */
export function onManual(cb: (id: SourceId) => void): () => void {
  manualListeners.add(cb);
  return () => manualListeners.delete(cb);
}

function mix(): Sticks {
  const id = pick(sources, performance.now());
  if (id === 'ai') input.source = 'ai';
  return id ? sources.get(id)!.sticks : NEUTRAL;
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
