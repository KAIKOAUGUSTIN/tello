// Teclado: W/S altura, A/D giro, setas frente/trás/lados (como o modo 2).
// Os valores sobem em rampa para o drone não dar trancos.

import { settings } from '../lib/settings.svelte';
import { report } from './mixer.svelte';
import { toDrone, type Axis2 } from './mapping';

export type Shortcut = 'takeoff' | 'land' | 'stop' | 'photo' | 'emergency';

const RAMP_PER_S = 3.5;

const keys = new Set<string>();
const left: Axis2 = { x: 0, y: 0 };
const right: Axis2 = { x: 0, y: 0 };

function target(neg: string, pos: string): number {
  return (keys.has(pos) ? 1 : 0) - (keys.has(neg) ? 1 : 0);
}

function approach(v: number, t: number, step: number): number {
  if (t === 0) return 0; // soltar a tecla para na hora
  return v < t ? Math.min(t, v + step) : Math.max(t, v - step);
}

function isTyping(e: KeyboardEvent): boolean {
  const el = e.target as HTMLElement | null;
  return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
}

const STICK_KEYS = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight']);

export function startKeyboard(onShortcut: (s: Shortcut, holdStart: boolean) => void): () => void {
  let lastEsc = 0;
  const down = (e: KeyboardEvent) => {
    if (isTyping(e) || e.ctrlKey || e.metaKey || e.altKey) return;
    if (STICK_KEYS.has(e.code)) {
      keys.add(e.code);
      e.preventDefault();
      return;
    }
    if (e.repeat) return;
    switch (e.code) {
      case 'KeyT':
        onShortcut('takeoff', true);
        break;
      case 'KeyL':
        onShortcut('land', true);
        break;
      case 'Space':
        e.preventDefault();
        onShortcut('stop', true);
        break;
      case 'KeyP':
        onShortcut('photo', true);
        break;
      case 'Escape': {
        // Esc duas vezes em menos de 1 s = emergência.
        const now = performance.now();
        if (now - lastEsc < 1000) onShortcut('emergency', true);
        lastEsc = now;
        break;
      }
    }
  };
  const up = (e: KeyboardEvent) => {
    keys.delete(e.code);
    if (e.code === 'KeyT') onShortcut('takeoff', false);
    if (e.code === 'KeyL') onShortcut('land', false);
  };
  const clear = () => keys.clear();

  let last = performance.now();
  const timer = window.setInterval(() => {
    const now = performance.now();
    const step = ((now - last) / 1000) * RAMP_PER_S;
    last = now;
    left.x = approach(left.x, target('KeyA', 'KeyD'), step);
    left.y = approach(left.y, target('KeyS', 'KeyW'), step);
    right.x = approach(right.x, target('ArrowLeft', 'ArrowRight'), step);
    right.y = approach(right.y, target('ArrowDown', 'ArrowUp'), step);
    // Teclado segue sempre o modo 2 (W/S = altura), independentemente do controle.
    report('keyboard', toDrone(left, right, 2, settings.invertPitch));
  }, 20);

  window.addEventListener('keydown', down);
  window.addEventListener('keyup', up);
  window.addEventListener('blur', clear);
  return () => {
    clearInterval(timer);
    window.removeEventListener('keydown', down);
    window.removeEventListener('keyup', up);
    window.removeEventListener('blur', clear);
  };
}
