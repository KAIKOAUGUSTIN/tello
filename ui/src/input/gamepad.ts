// Controle Xbox (ou qualquer controle com o "standard mapping" da Gamepad API).
//
// Sticks (modo 2): esquerdo = altura/giro, direito = frente/lado.
// Menu (segurar 1 s) = decolar · Menu no ar = pousar · View + Menu = emergência
// A = foto · B = pairar · Y = modo rápido · LB + D-pad = flips (diagonais
// apertando duas direções) · RB = Throw & Go no chão / pouso na palma no ar.

import {
  beginTakeoffHold,
  emergency,
  endTakeoffHold,
  flip,
  hover,
  land,
  palmLand,
  photo,
  throwAndGo,
  toggleFastMode,
} from '../lib/commands.svelte';
import { isFlying, onLowBattery } from '../lib/drone.svelte';
import { settings } from '../lib/settings.svelte';
import { toast } from '../lib/toasts.svelte';
import type { FlipDir } from '../lib/types';
import { input, report } from './mixer.svelte';
import { shape, toDrone } from './mapping';
import { NEUTRAL } from '../lib/types';

const B = { A: 0, B: 1, X: 2, Y: 3, LB: 4, RB: 5, LT: 6, RT: 7, VIEW: 8, MENU: 9, UP: 12, DOWN: 13, LEFT: 14, RIGHT: 15 };

let prev: boolean[] = [];
let activeIndex: number | null = null;

function pressed(pad: Gamepad, i: number): boolean {
  return pad.buttons[i]?.pressed ?? false;
}

function rumble(pad: Gamepad | null, ms: number, strong: number, weak: number): void {
  if (!pad || !settings.rumble) return;
  const actuator = (pad as Gamepad & { vibrationActuator?: GamepadHapticActuator }).vibrationActuator;
  actuator?.playEffect?.('dual-rumble', { duration: ms, strongMagnitude: strong, weakMagnitude: weak }).catch(() => {});
}

function currentPad(): Gamepad | null {
  const pads = navigator.getGamepads?.() ?? [];
  if (activeIndex !== null && pads[activeIndex]) return pads[activeIndex];
  for (const p of pads) {
    if (p?.connected) {
      activeIndex = p.index;
      return p;
    }
  }
  return null;
}

function flipDirection(pad: Gamepad): FlipDir | null {
  const up = pressed(pad, B.UP);
  const down = pressed(pad, B.DOWN);
  const left = pressed(pad, B.LEFT);
  const right = pressed(pad, B.RIGHT);
  if (up && left) return 'forward_left';
  if (up && right) return 'forward_right';
  if (down && left) return 'back_left';
  if (down && right) return 'back_right';
  if (up) return 'forward';
  if (down) return 'back';
  if (left) return 'left';
  if (right) return 'right';
  return null;
}

function poll(): void {
  const pad = currentPad();
  if (!pad) {
    report('gamepad', NEUTRAL);
    return;
  }
  const dz = settings.deadzone;
  const ex = settings.expo;
  const axis = (i: number) => shape(pad.axes[i] ?? 0, dz, ex);
  // Na Gamepad API o eixo Y é positivo para baixo.
  const left = { x: axis(0), y: -axis(1) };
  const right = { x: axis(2), y: -axis(3) };
  report('gamepad', toDrone(left, right, settings.stickMode, settings.invertPitch));

  const now = pad.buttons.map((b) => b.pressed);
  const edge = (i: number) => now[i] && !prev[i];
  const released = (i: number) => !now[i] && prev[i];

  if (now[B.VIEW] && now[B.MENU] && (edge(B.VIEW) || edge(B.MENU))) {
    endTakeoffHold();
    emergency();
    rumble(pad, 600, 1, 1);
  } else if (edge(B.MENU)) {
    if (isFlying()) land();
    else beginTakeoffHold();
  }
  if (released(B.MENU)) endTakeoffHold();

  if (edge(B.A)) photo();
  if (edge(B.B)) hover();
  if (edge(B.Y)) toggleFastMode();
  if (edge(B.RB)) (isFlying() ? palmLand : throwAndGo)();

  const dpadEdge = edge(B.UP) || edge(B.DOWN) || edge(B.LEFT) || edge(B.RIGHT);
  if (now[B.LB] && dpadEdge) {
    // Pequena espera para capturar diagonais (duas direções quase juntas).
    setTimeout(() => {
      const p = currentPad();
      const dir = p && flipDirection(p);
      if (dir) {
        flip(dir);
        rumble(p, 150, 0.4, 0.6);
      }
    }, 60);
  }
  prev = now;
}

export function startGamepad(): () => void {
  const onConnect = (e: GamepadEvent) => {
    activeIndex = e.gamepad.index;
    input.gamepad = cleanName(e.gamepad.id);
    toast('ok', 'Controle conectado', cleanName(e.gamepad.id));
    rumble(e.gamepad, 200, 0.3, 0.5);
  };
  const onDisconnect = (e: GamepadEvent) => {
    if (e.gamepad.index === activeIndex) {
      activeIndex = null;
      input.gamepad = null;
      prev = [];
      report('gamepad', NEUTRAL);
      endTakeoffHold();
      toast('warn', 'Controle desconectado', isFlying() ? 'O drone está pairando no lugar.' : undefined);
    }
  };
  window.addEventListener('gamepadconnected', onConnect);
  window.addEventListener('gamepaddisconnected', onDisconnect);
  const offBattery = onLowBattery((level) =>
    level === 'critical' ? rumble(currentPad(), 900, 1, 1) : rumble(currentPad(), 400, 0.6, 0.6),
  );
  const timer = window.setInterval(poll, 16);
  return () => {
    clearInterval(timer);
    offBattery();
    window.removeEventListener('gamepadconnected', onConnect);
    window.removeEventListener('gamepaddisconnected', onDisconnect);
  };
}

function cleanName(id: string): string {
  // "Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)" → "Xbox Wireless Controller"
  return id.replace(/\s*\(.*\)\s*$/, '') || id;
}
