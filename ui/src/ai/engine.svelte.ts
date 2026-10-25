// Motor de visão: manda quadros do vídeo para o worker do MediaPipe e repassa
// o que ele achou (detecções, o alvo rastreado e a mão). Só analisa enquanto a
// IA está ligada e o vídeo chega.

import { iou, type Box, type Detection } from './boxes.ts';
import type { HandSeen } from './hand.ts';
import type { FromWorker, HandCrop, ToWorker, TrackCommand } from './messages.ts';
import { mockHand, mockScene, type MockObject } from './mock.ts';
import type { Motion } from './motion.ts';
import type { Seen } from './tracker.ts';

export type AiStatus = 'off' | 'loading' | 'ready' | 'error';

export const ai = $state({
  status: 'off' as AiStatus,
  /** 'GPU', 'CPU' ou 'simulada' (?ai=mock). */
  delegate: null as string | null,
  /** Análises concluídas no último segundo. */
  hz: 0,
  /** Duração da última análise. */
  ms: 0,
  /** Coisas que dá para seguir, da última vez que o detector rodou. */
  detections: [] as Detection[],
  /** `ts` do quadro dessas detecções. */
  detectionsTs: 0,
  /** Largura dividida pela altura do quadro analisado. */
  aspect: 4 / 3,
  /** `performance.now()` do último resultado. */
  at: 0,
  error: null as string | null,
  /** Reconhecedor de gestos (carrega só quando os gestos são ligados). */
  hands: 'off' as AiStatus,
  handsError: null as string | null,
  /** Só no motor simulado: o que está "na frente da câmera", para desenhar. */
  mockObjects: [] as MockObject[],
});

/** Resultado de uma análise, para quem acompanha o alvo. */
export interface AiResult {
  /** `performance.now()` de quando o quadro chegou à IA. */
  ts: number;
  /** `null` quando o detector não rodou neste quadro. */
  detections: Detection[] | null;
  track: Seen | null;
  /** Giro da câmera desde o quadro rastreado anterior (`null`: sem medida). */
  motion: Motion | null;
  /** `null` quando a mão não foi procurada neste quadro. */
  hand: HandSeen | null;
}

/** Sem rastrear, o detector roda em todo quadro analisado, até esse limite. */
const DETECT_HZ = 15;
/** Rastreando, o rastreador visual roda em todo quadro, até esse limite. */
const TRACK_HZ = 30;
/**
 * Rastreando, o detector corrige tamanho e deriva de tempos em tempos: no máximo
 * a cada DETECT_GAP_MS e, se ele for lento, espaçado para o rastreador ficar com
 * a maior parte do tempo (DETECT_SHARE vezes a duração dele). Se o rastreador
 * vacila (PSR baixo), roda já no intervalo mínimo.
 */
const DETECT_GAP_MS = 100;
const DETECT_SHARE = 3;
const SHAKY_PSR = 10;
/** Com os gestos ligados, a mão é procurada até tantas vezes por segundo, alternando com o detector. */
const HAND_HZ = 10;
const HAND_CROP_PX = 256;
const ANALYSIS_WIDTH = 480;
/** Sem resultado novo por este tempo, as detecções são descartadas. */
const STALE_MS = 500;

let worker: Worker | null = null;
let mock = false;
let busy = false;
let lastSent = 0;
let done: number[] = [];
let sweep: number | undefined;
let tracking = false;
let pendingInit: TrackCommand['init'] = null;
let lastDetect = 0;
let detectMs = 0;
let lastPsr = Infinity;
/** Onde procurar a mão (fração do quadro), ou `null`. */
let handRegion: Box | null = null;
let lastHand = 0;
let lastWasDetect = false;
let handsSince = 0;
const listeners = new Set<(r: AiResult) => void>();

function mockRequested(): boolean {
  return typeof location !== 'undefined' && new URLSearchParams(location.search).get('ai') === 'mock';
}

/** Avisa a cada análise concluída. */
export function onResult(cb: (r: AiResult) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/**
 * Começa (ou recomeça) o rastreador visual nesta caixa, a partir do próximo
 * quadro. `ts`: o quadro em que ela foi vista (sem ele, o mais recente).
 */
export function startTracking(box: Box, ts = performance.now()): void {
  tracking = true;
  pendingInit = { box, ts };
  lastDetect = performance.now();
  lastPsr = Infinity;
}

export function stopTracking(): void {
  tracking = false;
  pendingInit = null;
  mockTarget = null;
}

/** Carrega o reconhecedor de gestos (avisa em `ai.hands`). */
export function startHands(): void {
  if (ai.hands === 'loading' || ai.hands === 'ready') return;
  ai.hands = 'loading';
  ai.handsError = null;
  handsSince = performance.now();
  if (mock) {
    setTimeout(() => {
      if (ai.hands === 'loading') ai.hands = 'ready';
    }, 300);
    return;
  }
  post({ type: 'hands', on: true });
}

export function stopHands(): void {
  if (ai.hands !== 'off') post({ type: 'hands', on: false });
  ai.hands = 'off';
  ai.handsError = null;
  handRegion = null;
}

/** Área do quadro onde procurar a mão (a parte de cima de quem comanda), ou `null`. */
export function setHandRegion(box: Box | null): void {
  handRegion = box;
}

export function startAi(): void {
  if (ai.status !== 'off' && ai.status !== 'error') return;
  ai.status = 'loading';
  ai.error = null;
  clearInterval(sweep);
  sweep = window.setInterval(expire, 250);
  if (mockRequested()) {
    mock = true;
    setTimeout(() => {
      if (!mock) return;
      ai.status = 'ready';
      ai.delegate = 'simulada';
    }, 600);
    return;
  }
  try {
    worker = new Worker(new URL('./vision.worker.ts', import.meta.url), { type: 'module' });
  } catch (e) {
    return fail(e instanceof Error ? e.message : String(e));
  }
  worker.onmessage = (e: MessageEvent<FromWorker>) => onMessage(e.data);
  worker.onerror = (e) => fail(e.message || 'o worker da IA parou');
  post({ type: 'init' });
}

export function stopAi(): void {
  worker?.terminate();
  worker = null;
  mock = false;
  busy = false;
  clearInterval(sweep);
  done = [];
  detectMs = 0;
  handRegion = null;
  stopTracking();
  Object.assign(ai, {
    status: 'off',
    delegate: null,
    hz: 0,
    ms: 0,
    detections: [],
    detectionsTs: 0,
    at: 0,
    error: null,
    hands: 'off',
    handsError: null,
    mockObjects: [],
  });
}

/** Chamado pelo player a cada quadro decodificado, antes de liberá-lo. */
export function analyzeFrame(frame: VideoFrame): void {
  if (ai.status !== 'ready' || busy) return;
  const now = performance.now();
  const handsOn = ai.hands === 'ready' && handRegion !== null;
  if (now - lastSent < 1000 / (tracking || handsOn ? TRACK_HZ : DETECT_HZ)) return;
  lastSent = now;
  const aspect = frame.displayWidth / frame.displayHeight;
  if (ai.aspect !== aspect) ai.aspect = aspect;

  const since = now - lastDetect;
  let detect =
    !tracking ||
    since >= Math.max(DETECT_GAP_MS, DETECT_SHARE * detectMs) ||
    (lastPsr < SHAKY_PSR && since >= DETECT_GAP_MS);
  // A mão alterna com o detector: os dois no mesmo quadro dobrariam o atraso.
  const region = handsOn && now - lastHand >= 1000 / HAND_HZ && (!detect || lastWasDetect) ? handRegion : null;
  if (region) {
    detect = false;
    lastHand = now;
  }
  if (detect) lastDetect = now;
  lastWasDetect = detect;
  const track: TrackCommand | null = tracking ? { init: pendingInit } : null;
  pendingInit = null;

  if (mock) return mockFrame(now, detect, track, region);
  if (!worker) return;
  busy = true;
  const copy = frame.clone();
  const height = Math.round(ANALYSIS_WIDTH / aspect);
  // Um recorte da mão que falhe não derruba o quadro; o que der certo e não for usado é liberado.
  Promise.allSettled([
    createImageBitmap(copy, { resizeWidth: ANALYSIS_WIDTH, resizeHeight: height, resizeQuality: 'low' }),
    region ? cropHand(copy, region) : null,
  ])
    .then(([small, crop]) => {
      const bitmap = small.status === 'fulfilled' ? small.value : null;
      const hand = crop.status === 'fulfilled' ? crop.value : null;
      if (!worker || !bitmap) {
        bitmap?.close();
        hand?.bitmap.close();
        busy = false;
        return;
      }
      post({ type: 'frame', bitmap, ts: now, detect, track, hand }, hand ? [bitmap, hand.bitmap] : [bitmap]);
    })
    .catch(() => (busy = false))
    .finally(() => copy.close());
}

/** Recorte da mão do quadro em resolução cheia (a mão a 3 m tem poucos pixels no quadro reduzido). */
async function cropHand(frame: VideoFrame, box: Box): Promise<HandCrop> {
  const fw = frame.displayWidth;
  const fh = frame.displayHeight;
  const sx = Math.round(box.x * fw);
  const sy = Math.round(box.y * fh);
  const sw = Math.max(1, Math.round(box.w * fw));
  const sh = Math.max(1, Math.round(box.h * fh));
  const bitmap = await createImageBitmap(frame, sx, sy, sw, sh, {
    resizeWidth: HAND_CROP_PX,
    resizeHeight: HAND_CROP_PX,
    resizeQuality: 'medium',
  });
  return { bitmap, box: { x: sx / fw, y: sy / fh, w: sw / fw, h: sh / fh } };
}

function post(msg: ToWorker, transfer: Transferable[] = []): void {
  worker?.postMessage(msg, transfer);
}

function onMessage(msg: FromWorker): void {
  if (msg.type === 'ready') {
    ai.status = 'ready';
    ai.delegate = msg.delegate;
    busy = false;
    detectMs = 0;
  } else if (msg.type === 'error') {
    fail(msg.message);
  } else if (msg.type === 'hands') {
    if (ai.hands !== 'loading' && ai.hands !== 'ready') return;
    ai.hands = msg.ok ? 'ready' : 'error';
    ai.handsError = msg.ok ? null : msg.message;
    if (!msg.ok) console.warn('IA: reconhecedor de gestos:', msg.message);
  } else {
    busy = false;
    finish({ ts: msg.ts, detections: msg.detections, track: msg.track, motion: msg.motion, hand: msg.hand }, msg.ms);
  }
}

// Motor simulado: as "detecções" e o "rastreador" saem da cena de mock.ts.
let mockTarget: { id: string | null; box: Box } | null = null;

function mockFrame(now: number, detect: boolean, track: TrackCommand | null, region: Box | null): void {
  const scene = mockScene(now);
  ai.mockObjects = scene;
  let seen: Seen | null = null;
  if (track?.init) {
    // Pega o objeto da cena sob a caixa marcada; sem nenhum, rastreia o "fundo" parado.
    let best: MockObject | null = null;
    const init = track.init.box;
    for (const o of scene) if (iou(o.box, init) > (best ? iou(best.box, init) : 0.15)) best = o;
    mockTarget = { id: best?.id ?? null, box: best?.box ?? init };
    seen = { box: mockTarget.box, psr: 99 };
  } else if (track && mockTarget) {
    const o = mockTarget.id ? scene.find((s) => s.id === mockTarget!.id) : null;
    if (o) mockTarget.box = o.box;
    seen = { box: mockTarget.box, psr: o || !mockTarget.id ? 20 : 3 };
  }
  const detections = detect
    ? scene.flatMap((o) => (o.category ? [{ ...o.box, category: o.category, score: 0.82 }] : []))
    : null;
  const person = scene.find((o) => o.category === 'person');
  const hand = region ? mockHand(now - handsSince, person && iou(person.box, region) > 0 ? person.box : null) : null;
  finish({ ts: now, detections, track: seen, motion: null, hand }, 3 + Math.random() * 2);
}

function finish(result: AiResult, ms: number): void {
  const now = performance.now();
  done.push(now);
  while (done.length && now - done[0] > 1000) done.shift();
  if (result.detections) {
    ai.detections = result.detections;
    ai.detectionsTs = result.ts;
    detectMs = detectMs ? detectMs * 0.7 + ms * 0.3 : ms;
  }
  if (result.track) lastPsr = result.track.psr;
  ai.at = now;
  ai.ms = Math.round(ms);
  ai.hz = done.length;
  listeners.forEach((cb) => cb(result));
}

function expire(): void {
  const now = performance.now();
  while (done.length && now - done[0] > 1000) done.shift();
  ai.hz = done.length;
  if (ai.detections.length && now - ai.at > STALE_MS) ai.detections = [];
}

function fail(message: string): void {
  console.warn('IA:', message);
  worker?.terminate();
  worker = null;
  busy = false;
  ai.status = 'error';
  ai.error = message;
  ai.detections = [];
  ai.hands = 'off';
  handRegion = null;
}
