// Motor de visão: manda quadros do vídeo para o worker do MediaPipe e guarda as
// poses encontradas. Só analisa enquanto a IA está ligada e o vídeo chega.

import type { FromWorker, ToWorker } from './messages.ts';
import { mockScene } from './mock.ts';
import type { Pose } from './pose.ts';

export type AiStatus = 'off' | 'loading' | 'ready' | 'error';

export const ai = $state({
  status: 'off' as AiStatus,
  /** 'GPU', 'CPU' ou 'simulada' (?ai=mock). */
  delegate: null as string | null,
  /** Análises concluídas no último segundo. */
  hz: 0,
  /** Duração da última análise. */
  ms: 0,
  poses: [] as Pose[],
  /** Largura dividida pela altura do quadro analisado. */
  aspect: 4 / 3,
  /** `performance.now()` do último resultado. */
  at: 0,
  error: null as string | null,
});

const MAX_HZ = 15;
const ANALYSIS_WIDTH = 480;
/** Sem resultado novo por este tempo, as poses são descartadas. */
const STALE_MS = 500;

let worker: Worker | null = null;
let mock = false;
let busy = false;
let lastSent = 0;
let done: number[] = [];
let sweep: number | undefined;
const listeners = new Set<() => void>();

function mockRequested(): boolean {
  return typeof location !== 'undefined' && new URLSearchParams(location.search).get('ai') === 'mock';
}

/** Avisa a cada resultado novo (inclusive quando as poses expiram). */
export function onResult(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
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
  Object.assign(ai, { status: 'off', delegate: null, hz: 0, ms: 0, poses: [], at: 0, error: null });
  notify();
}

/** Chamado pelo player a cada quadro decodificado, antes de liberá-lo. */
export function analyzeFrame(frame: VideoFrame): void {
  if (ai.status !== 'ready' || busy) return;
  const now = performance.now();
  if (now - lastSent < 1000 / MAX_HZ) return;
  lastSent = now;
  const aspect = frame.displayWidth / frame.displayHeight;
  if (ai.aspect !== aspect) ai.aspect = aspect;
  if (mock) return finish(mockScene(now), 4 + Math.random() * 3);
  if (!worker) return;
  busy = true;
  const copy = frame.clone();
  const height = Math.round(ANALYSIS_WIDTH / aspect);
  createImageBitmap(copy, { resizeWidth: ANALYSIS_WIDTH, resizeHeight: height, resizeQuality: 'low' })
    .then((bitmap) => {
      if (!worker) {
        bitmap.close();
        busy = false;
        return;
      }
      post({ type: 'frame', bitmap, ts: now }, [bitmap]);
    })
    .catch(() => (busy = false))
    .finally(() => copy.close());
}

function post(msg: ToWorker, transfer: Transferable[] = []): void {
  worker?.postMessage(msg, transfer);
}

function onMessage(msg: FromWorker): void {
  if (msg.type === 'ready') {
    ai.status = 'ready';
    ai.delegate = msg.delegate;
    busy = false;
  } else if (msg.type === 'error') {
    fail(msg.message);
  } else {
    busy = false;
    finish(msg.poses, msg.ms);
  }
}

function finish(poses: Pose[], ms: number): void {
  const now = performance.now();
  done.push(now);
  while (done.length && now - done[0] > 1000) done.shift();
  ai.poses = poses;
  ai.at = now;
  ai.ms = Math.round(ms);
  ai.hz = done.length;
  notify();
}

function expire(): void {
  const now = performance.now();
  while (done.length && now - done[0] > 1000) done.shift();
  ai.hz = done.length;
  if (ai.poses.length && now - ai.at > STALE_MS) {
    ai.poses = [];
    notify();
  }
}

function fail(message: string): void {
  console.warn('IA:', message);
  worker?.terminate();
  worker = null;
  busy = false;
  ai.status = 'error';
  ai.error = message;
  ai.poses = [];
  notify();
}

function notify(): void {
  listeners.forEach((cb) => cb());
}
