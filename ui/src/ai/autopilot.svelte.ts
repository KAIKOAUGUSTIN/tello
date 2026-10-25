// Piloto da IA: trava uma pessoa e transforma as análises em sticks.
// Na dúvida, para: pessoa perdida ou análises atrasadas deixam o drone
// pairando, e qualquer sinal de problema (ou toque nos controles) desliga.

import { backendRef, drone, isFlying } from '../lib/drone.svelte';
import { platform } from '../lib/platform.svelte';
import { settings } from '../lib/settings.svelte';
import { toast, type ToastKind } from '../lib/toasts.svelte';
import { NEUTRAL, type Sticks } from '../lib/types';
import { input, onManual, report } from '../input/mixer.svelte';
import { ai, onResult } from './engine.svelte';
import { FollowController, slew } from './follow.ts';
import { distanceM } from './pose.ts';
import { lockTarget, Tracker, type Target } from './tracker.ts';

/** Abaixo disso o atraso fica grande demais para seguir alguém. */
export const MIN_HZ = 6;
/** Pessoa sumida por este tempo: o seguir desliga (o drone já estava pairando). */
const GIVE_UP_MS = 15_000;
/** Análises lentas por este tempo: desliga. */
const SLOW_GRACE_MS = 3000;
/** Variação máxima dos sticks da IA por segundo. */
const SLEW_PER_S = 1;
const WATCH_MS = 100;

export const autopilot = $state({
  follow: false,
  /** Pessoa travada, como vista na última análise em que apareceu. */
  target: null as Target | null,
  /** A pessoa não aparece na última análise. */
  lost: false,
  distanceM: null as number | null,
});

let tracker: Tracker | null = null;
let controller: FollowController | null = null;
let out: Sticks = NEUTRAL;
let outAt = 0;
let slowSince: number | null = null;
let stops: (() => void)[] = [];

function note(text: string): void {
  void backendRef()
    ?.logNote(`IA: ${text}`)
    .catch(() => {});
}

/** Motivo para não poder ligar agora, ou `null`. */
function blocker(): string | null {
  if (!settings.aiEnabled) return 'Ative a IA em Configurações';
  if (ai.status !== 'ready') return ai.status === 'loading' ? 'A IA ainda está carregando' : 'A IA não está funcionando';
  if (drone.conn !== 'connected') return 'Conecte ao drone';
  if (!isFlying()) return 'Decole antes';
  if (!drone.videoOn) return 'Ligue o vídeo';
  if (ai.hz < MIN_HZ) return `Análises lentas demais (${ai.hz}/s, mínimo ${MIN_HZ})`;
  return null;
}

export function toggleFollow(): void {
  if (autopilot.follow) return stopAutopilot(undefined, 'info');
  const why = blocker();
  if (why) return void toast('warn', 'Não deu para seguir', why);
  const lock = lockTarget(ai.poses, ai.aspect);
  if (!lock.ok) return void toast('warn', 'Não deu para seguir', lock.reason);

  tracker = new Tracker(lock.target, ai.aspect);
  controller = new FollowController(lock.target.len);
  out = NEUTRAL;
  outAt = performance.now();
  slowSince = null;
  Object.assign(autopilot, {
    follow: true,
    target: lock.target,
    lost: false,
    distanceM: distanceM(lock.target.len),
  });
  stops = [
    onResult(step),
    onManual(() => stopAutopilot('Controle manual', 'info')),
    (() => {
      const timer = window.setInterval(watch, WATCH_MS);
      return () => clearInterval(timer);
    })(),
  ];
  const d = distanceM(lock.target.len).toLocaleString('pt-BR', { maximumFractionDigits: 1 });
  toast('ok', 'Seguindo', `Mantendo uns ${d} m. Qualquer toque nos controles devolve o comando.`);
  note(`seguir ligado (~${d} m)`);
}

/** Desliga a IA e devolve o comando (sticks no centro). Sem motivo = pedido do usuário. */
export function stopAutopilot(reason?: string, kind: ToastKind = 'warn'): void {
  if (!autopilot.follow) return;
  stops.forEach((s) => s());
  stops = [];
  tracker = null;
  controller = null;
  out = NEUTRAL;
  report('ai', NEUTRAL);
  Object.assign(autopilot, { follow: false, target: null, lost: false, distanceM: null });
  toast(kind, 'Seguir desligado', reason);
  note(`seguir desligado${reason ? `: ${reason}` : ''}`);
}

/** A cada análise: acha a pessoa e calcula os sticks. */
function step(): void {
  if (!tracker || !controller) return;
  const now = performance.now();
  const target = tracker.update(ai.poses, now);
  if (target) {
    autopilot.target = target;
    autopilot.distanceM = distanceM(target.len);
  }
  autopilot.lost = !target;
  if (tracker.lostFor(now) > GIVE_UP_MS) return stopAutopilot('A pessoa sumiu da imagem');

  const follow = controller.step(target, drone.telemetry?.fast_mode === true);
  const next: Sticks = { roll: 0, pitch: follow.pitch, throttle: 0, yaw: follow.yaw };
  const dt = Math.min(0.5, (now - outAt) / 1000);
  out = slew(out, next, SLEW_PER_S * dt);
  outAt = now;
  report('ai', out);
}

/** Confere o que não depende das análises. */
function watch(): void {
  const now = performance.now();
  if (!settings.aiEnabled) return stopAutopilot('IA desligada');
  if (ai.status !== 'ready') return stopAutopilot('A IA parou');
  if (drone.conn !== 'connected') return stopAutopilot('Conexão encerrada');
  if (drone.linkLost || drone.reconnecting) return stopAutopilot('Sinal do drone perdido');
  if (!isFlying()) return stopAutopilot('O drone não está voando');
  if (!drone.videoOn) return stopAutopilot('Vídeo desligado');
  if (!input.focused) return stopAutopilot(platform.mobile ? 'App em segundo plano' : 'Janela sem foco');
  if (ai.hz < MIN_HZ) {
    slowSince ??= now;
    if (now - slowSince > SLOW_GRACE_MS) return stopAutopilot(`Análises lentas demais (${ai.hz}/s)`);
  } else {
    slowSince = null;
  }
}
