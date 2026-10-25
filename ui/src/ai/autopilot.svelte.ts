// Piloto da IA: o usuário escolhe o que seguir (tocando numa caixa cinza ou
// arrastando em volta), o rastreador acompanha e o controle vira sticks. No
// chão só rastreia; no ar, segue. Na dúvida, para: alvo perdido ou análises
// atrasadas deixam o drone pairando, e qualquer sinal de problema (ou toque
// nos controles) desliga.

import { backendRef, drone, isFlying } from '../lib/drone.svelte';
import { platform } from '../lib/platform.svelte';
import { settings } from '../lib/settings.svelte';
import { toast, type ToastKind } from '../lib/toasts.svelte';
import { NEUTRAL, type Sticks } from '../lib/types';
import { input, onManual } from '../input/mixer.svelte';
import { distanceM, matchRect, pickAt, type Box } from './boxes.ts';
import { ai, onResult, startTracking, stopTracking, type AiResult } from './engine.svelte';
import { FOLLOW, FollowController, slew } from './follow.ts';
import { reportAi } from './output';
import { Tracker } from './tracker.ts';

/** Abaixo disso o atraso fica grande demais para seguir alguma coisa. */
export const MIN_HZ = 10;
/** Alvo sumido por este tempo: o seguir desliga (o drone já estava pairando). */
const GIVE_UP_MS = 15_000;
/** Análises lentas por este tempo: desliga. */
const SLOW_GRACE_MS = 3000;
/** Arrastes menores que isso (fração do quadro) contam como toque. */
const MIN_DRAG = 0.03;
const WATCH_MS = 100;

export const autopilot = $state({
  /** Escolhendo o que seguir (as caixas cinza aparecem na tela). */
  selecting: false,
  /** Alvo travado: rastreando no chão, seguindo no ar. */
  follow: false,
  /** O drone está de fato se movendo atrás do alvo. */
  active: false,
  label: null as string | null,
  /** Categoria do COCO do alvo, ou `null` para algo marcado à mão. */
  category: null as string | null,
  /** Caixa do alvo, como vista por último (perdido, levada pelo giro da câmera). */
  box: null as Box | null,
  /** O alvo não aparece na última análise. */
  lost: false,
  distanceM: null as number | null,
});

let tracker: Tracker | null = null;
let controller: FollowController | null = null;
let out: Sticks = NEUTRAL;
let outAt = 0;
let lastStep = 0;
let slowSince: number | null = null;
let stops: (() => void)[] = [];

function note(text: string): void {
  void backendRef()
    ?.logNote(`IA: ${text}`)
    .catch(() => {});
}

/** Motivo para a IA não poder comandar agora (seguir ou gestos), ou `null`. */
export function aiBlocker(): string | null {
  if (!settings.aiEnabled) return 'Ative a IA em Configurações';
  if (ai.status !== 'ready') return ai.status === 'loading' ? 'A IA ainda está carregando' : 'A IA não está funcionando';
  if (drone.conn !== 'connected') return 'Conecte ao drone';
  if (!drone.videoOn) return 'Ligue o vídeo';
  return null;
}

/** Botão Seguir e tecla F: abre a escolha, ou cancela/desliga o que estiver ligado. */
export function toggleFollow(): void {
  if (autopilot.follow) return stopAutopilot(undefined, 'info');
  if (autopilot.selecting) return void (autopilot.selecting = false);
  const why = aiBlocker();
  if (why) return void toast('warn', 'Não deu para seguir', why);
  autopilot.selecting = true;
}

/** Toque na tela durante a escolha (coordenadas em fração do quadro). */
export function selectAt(x: number, y: number): void {
  const d = pickAt(ai.detections, x, y);
  if (!d) return void toast('info', 'Nada aqui', 'Toque numa caixa cinza ou arraste em volta do que quer seguir.');
  lock(d, d.category, ai.detectionsTs);
}

/** Área arrastada durante a escolha: casa com uma detecção ou vira um objeto genérico. */
export function selectRect(r: Box): void {
  if (r.w < MIN_DRAG || r.h < MIN_DRAG) return selectAt(r.x + r.w / 2, r.y + r.h / 2);
  const d = matchRect(ai.detections, r);
  // A área foi marcada sobre o vídeo de agora; a detecção é de um quadro anterior.
  if (d) lock(d, d.category, ai.detectionsTs);
  else lock(r, null, performance.now());
}

/** `seenAt`: `ts` do quadro em que a caixa foi vista. */
function lock(box: Box, category: string | null, seenAt: number): void {
  const now = performance.now();
  tracker = new Tracker(box, category, now, ai.aspect);
  controller = null;
  out = NEUTRAL;
  outAt = lastStep = now;
  slowSince = null;
  startTracking(box, seenAt);
  Object.assign(autopilot, {
    selecting: false,
    follow: true,
    active: false,
    label: tracker.label,
    category,
    box,
    lost: false,
    distanceM: distanceM(box, category),
  });
  stops = [
    onResult(step),
    onManual(() => stopAutopilot('Controle manual', 'info')),
    (() => {
      const timer = window.setInterval(watch, WATCH_MS);
      return () => clearInterval(timer);
    })(),
  ];
  note(`alvo travado: ${tracker.label}`);
  if (isFlying()) activate();
  else toast('ok', `Alvo travado: ${tracker.label}`, 'Decole para seguir. Qualquer toque nos controles devolve o comando.');
}

/** No ar e com o alvo visível: passa a seguir, mantendo o tamanho de agora. */
function activate(): void {
  if (!tracker) return;
  controller = new FollowController(tracker.estimate().size);
  autopilot.active = true;
  const d = autopilot.distanceM;
  const where = d ? `Mantendo uns ${d.toLocaleString('pt-BR', { maximumFractionDigits: 1 })} m. ` : '';
  toast('ok', `Seguindo: ${tracker.label}`, `${where}Qualquer toque nos controles devolve o comando.`);
  note(`seguir ligado: ${tracker.label}${d ? ` (~${d.toFixed(1)} m)` : ''}`);
}

/** Desliga a IA e devolve o comando (sticks no centro). Sem motivo = pedido do usuário. */
export function stopAutopilot(reason?: string, kind: ToastKind = 'warn'): void {
  autopilot.selecting = false;
  if (!autopilot.follow) return;
  const wasActive = autopilot.active;
  stops.forEach((s) => s());
  stops = [];
  stopTracking();
  tracker = null;
  controller = null;
  out = NEUTRAL;
  reportAi('follow', NEUTRAL);
  Object.assign(autopilot, {
    follow: false,
    active: false,
    label: null,
    category: null,
    box: null,
    lost: false,
    distanceM: null,
  });
  // Pedido do próprio usuário dispensa aviso (o botão já mostra); no celular o
  // aviso ainda cobriria o vídeo na hora de escolher outro alvo.
  if (reason) toast(kind, wasActive ? 'Seguir desligado' : 'Alvo solto', reason);
  note(`seguir desligado${reason ? `: ${reason}` : ''}`);
}

/** A cada análise: atualiza o alvo e, no ar, calcula os sticks. */
function step(r: AiResult): void {
  if (!tracker) return;
  const now = performance.now();
  const seen = tracker.update(r.track, r.detections, r.ts, r.motion);
  if (tracker.reinit) startTracking(tracker.reinit, r.ts);
  autopilot.box = tracker.box;
  if (seen) autopilot.distanceM = distanceM(seen, tracker.category);
  autopilot.lost = !seen;
  if (tracker.lostFor(r.ts) > GIVE_UP_MS) return stopAutopilot('O alvo sumiu da imagem');
  if (!isFlying()) return;
  if (!controller) {
    if (!seen) return;
    activate();
  }

  const dt = Math.min(0.5, (now - lastStep) / 1000);
  lastStep = now;
  const est = seen ? tracker.estimate() : null;
  const follow = controller!.step(est, {
    age: (now - r.ts) / 1000,
    bottom: tracker.box.y + tracker.box.h,
    dt,
    fast: drone.telemetry?.fast_mode === true,
  });
  const next: Sticks = { roll: 0, pitch: follow.pitch, throttle: 0, yaw: follow.yaw };
  out = slew(out, next, FOLLOW.slewPerS * Math.min(0.5, (now - outAt) / 1000));
  outAt = now;
  reportAi('follow', out);
}

/** Confere o que não depende das análises. */
function watch(): void {
  const now = performance.now();
  if (!settings.aiEnabled) return stopAutopilot('IA desligada');
  if (ai.status !== 'ready') return stopAutopilot('A IA parou');
  if (drone.conn !== 'connected') return stopAutopilot('Conexão encerrada');
  if (drone.linkLost || drone.reconnecting) return stopAutopilot('Sinal do drone perdido');
  if (!drone.videoOn) return stopAutopilot('Vídeo desligado');
  if (!input.focused) return stopAutopilot(platform.mobile ? 'App em segundo plano' : 'Janela sem foco');
  if (!autopilot.active) return;
  if (!isFlying()) return stopAutopilot('O drone não está voando');
  if (ai.hz < MIN_HZ) {
    slowSince ??= now;
    if (now - slowSince > SLOW_GRACE_MS) return stopAutopilot(`Análises lentas demais (${ai.hz}/s)`);
  } else {
    slowSince = null;
  }
}
