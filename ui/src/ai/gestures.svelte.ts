// Gestos com a mão (IA): V tira foto depois de 3 s; palma aberta guia o drone
// no ar (sobe, desce e vai para os lados com a mão); punho fechado para tudo.
// Quem comanda é a pessoa seguida ou, sem seguir, a maior pessoa perto do
// centro. O controle manual ganha de tudo, e nenhum gesto decola ou pousa.

import { photo } from '../lib/commands.svelte';
import { backendRef, drone, isFlying } from '../lib/drone.svelte';
import { platform } from '../lib/platform.svelte';
import { settings } from '../lib/settings.svelte';
import { toast } from '../lib/toasts.svelte';
import { NEUTRAL, type Sticks } from '../lib/types';
import { input, onManual } from '../input/mixer.svelte';
import { aiBlocker, autopilot, stopAutopilot } from './autopilot.svelte';
import type { Box } from './boxes.ts';
import { ai, onResult, setHandRegion, startHands, stopHands, type AiResult } from './engine.svelte';
import { FOLLOW, slew } from './follow.ts';
import { GestureFilter, handCrop, PalmGuide, PhotoTrigger, pickCommander, type Gesture, type Point } from './hand.ts';
import { reportAi } from './output';

const WATCH_MS = 100;
/** O detector às vezes perde a pessoa num quadro: quem comanda continua o mesmo por esse tempo. */
const COMMANDER_HOLD_MS = 1000;

export const gestures = $state({
  on: false,
  /** Gesto valendo agora (depois do filtro). */
  gesture: 'none' as Gesture,
  /** Última palma vista, para o marcador sobre o vídeo. */
  palm: null as Point | null,
  /** A palma está guiando o drone. */
  guiding: false,
  /** Segundos até a foto do V, ou `null`. */
  countdown: null as number | null,
});

let filter = new GestureFilter();
let trigger = new PhotoTrigger();
let guide: PalmGuide | null = null;
/** Caixa de quem comanda com a mão. */
let commander: Box | null = null;
let commanderAt = 0;
/** Depois de um comando manual, a palma precisa sair e voltar para guiar de novo. */
let needRelease = false;
let out: Sticks = NEUTRAL;
let outAt = 0;
let stops: (() => void)[] = [];

function note(text: string): void {
  void backendRef()
    ?.logNote(`IA: ${text}`)
    .catch(() => {});
}

/** Botão Gestos e tecla G. */
export function toggleGestures(): void {
  if (gestures.on) return stopGestures();
  const why = aiBlocker();
  if (why) return void toast('warn', 'Não deu para ligar os gestos', why);
  filter = new GestureFilter();
  trigger = new PhotoTrigger();
  guide = null;
  commander = null;
  needRelease = false;
  Object.assign(gestures, { on: true, gesture: 'none', palm: null, guiding: false, countdown: null });
  startHands();
  stops = [
    onResult(step),
    onManual(interruptGestures),
    (() => {
      const timer = window.setInterval(watch, WATCH_MS);
      return () => clearInterval(timer);
    })(),
  ];
  note('gestos ligados');
  toast('ok', 'Gestos ligados', 'V: foto em 3 s. Palma aberta: guia o drone no ar. Punho fechado: para tudo.');
}

/** Desliga os gestos. Sem motivo = pedido do usuário. */
export function stopGestures(reason?: string): void {
  if (!gestures.on) return;
  stops.forEach((s) => s());
  stops = [];
  endGuide();
  trigger.cancel();
  stopHands();
  setHandRegion(null);
  Object.assign(gestures, { on: false, gesture: 'none', palm: null, guiding: false, countdown: null });
  if (reason) toast('warn', 'Gestos desligados', reason);
  note(`gestos desligados${reason ? `: ${reason}` : ''}`);
}

/** Controle manual, pairar, pousar ou emergência: larga a guia e cancela a foto (os gestos continuam ligados). */
export function interruptGestures(): void {
  if (!gestures.on) return;
  if (trigger.counting) {
    trigger.cancel();
    gestures.countdown = null;
    toast('info', 'Foto cancelada', 'O controle voltou para você.');
    note('foto do V cancelada');
  }
  if (guide) {
    needRelease = true;
    endGuide();
  }
}

function heightCm(): number | null {
  const t = drone.telemetry;
  return t?.tof_cm ?? t?.height_cm ?? null;
}

/** Quem comanda: a pessoa seguida; senão, a maior perto do centro (a mesma enquanto der). */
function updateCommander(r: AiResult): void {
  if (autopilot.follow && autopilot.category === 'person' && autopilot.box) {
    commander = autopilot.box;
    commanderAt = r.ts;
  } else if (r.detections) {
    const found = pickCommander(r.detections, commander);
    if (found) [commander, commanderAt] = [found, r.ts];
    else if (r.ts - commanderAt > COMMANDER_HOLD_MS) commander = null;
  }
  setHandRegion(commander ? handCrop(commander, ai.aspect) : null);
}

function step(r: AiResult): void {
  updateCommander(r);
  // Ninguém na imagem: a mão deixa de ser procurada, então o gesto zera aqui.
  if (!commander) return forget();
  if (!r.hand) return;
  const before = filter.current;
  const stable = filter.update(r.hand.gesture, r.ts);
  gestures.gesture = stable;
  gestures.palm = r.hand.palm;
  if (stable !== 'palm') needRelease = false;
  if (stable === 'fist') {
    if (before !== 'fist') fist();
    return;
  }

  if (trigger.see(stable, filter.since, r.ts)) {
    gestures.countdown = trigger.remaining(r.ts);
    note('V: foto em 3 s');
  }

  if (stable === 'palm' && isFlying() && !needRelease && commander) {
    if (!guide && r.hand.palm) startGuide(r.hand.palm, commander);
    else if (guide) guideStep(r.hand.palm, commander);
  } else {
    endGuide();
  }
}

function forget(): void {
  if (filter.current === 'none' && !gestures.palm && !guide) return;
  filter = new GestureFilter();
  gestures.gesture = 'none';
  gestures.palm = null;
  needRelease = false;
  endGuide();
}

/** Punho fechado: para a guia, a foto e o seguir; o drone fica pairando. */
function fist(): void {
  const busy = !!guide || trigger.counting || autopilot.follow;
  trigger.cancel();
  gestures.countdown = null;
  endGuide();
  if (autopilot.follow) stopAutopilot('Punho fechado', 'info');
  // No ar, sempre confirma (a palma pode ter soltado um instante antes do punho valer).
  else if (busy || isFlying()) toast('info', 'Parado', 'Punho fechado: o drone fica pairando.');
  note('punho fechado');
}

function startGuide(palm: Point, body: Box): void {
  guide = new PalmGuide(palm, body, ai.aspect);
  out = NEUTRAL;
  outAt = performance.now();
  gestures.guiding = true;
  note('mão: guiando');
}

function guideStep(palm: Point | null, body: Box): void {
  // Sem a mão neste quadro: paira até ela voltar (ou o gesto soltar).
  const s = palm ? guide!.step(palm, body, ai.aspect, heightCm()) : { roll: 0, throttle: 0 };
  const now = performance.now();
  out = slew(out, { roll: s.roll, pitch: 0, throttle: s.throttle, yaw: 0 }, FOLLOW.slewPerS * Math.min(0.5, (now - outAt) / 1000));
  outAt = now;
  reportAi('hand', out);
}

function endGuide(): void {
  if (!guide) return;
  guide = null;
  out = NEUTRAL;
  gestures.guiding = false;
  reportAi('hand', NEUTRAL);
  note('mão: guia solta');
}

/** Contagem da foto e o que não depende das análises. */
function watch(): void {
  const now = performance.now();
  if (!settings.aiEnabled) return stopGestures('IA desligada');
  if (ai.status !== 'ready') return stopGestures('A IA parou');
  if (ai.hands === 'error') return stopGestures(`O reconhecedor de mãos falhou: ${ai.handsError ?? 'erro desconhecido'}`);
  if (drone.conn !== 'connected') return stopGestures('Conexão encerrada');
  if (drone.linkLost || drone.reconnecting) return stopGestures('Sinal do drone perdido');
  if (!drone.videoOn) return stopGestures('Vídeo desligado');
  if (!input.focused) return stopGestures(platform.mobile ? 'App em segundo plano' : 'Janela sem foco');
  if (guide && !isFlying()) endGuide();
  if (trigger.tick(now)) {
    gestures.countdown = null;
    photo();
    note('foto pelo V');
  } else {
    gestures.countdown = trigger.remaining(now);
  }
}
