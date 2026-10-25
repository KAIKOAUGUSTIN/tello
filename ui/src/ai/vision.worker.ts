// Worker da IA: roda o detector de objetos do MediaPipe, o rastreador visual e
// (com os gestos ligados) o reconhecedor de mãos fora da thread da interface.
// Tudo vem de dentro do app (wasm e modelos), sem internet.

import { GestureRecognizer, ObjectDetector } from '@mediapipe/tasks-vision';
import wasmLoaderPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.js?url';
import wasmBinaryPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.wasm?url';

import { FOLLOWABLE, type Box, type Detection } from './boxes.ts';
import { toGesture, type HandSeen } from './hand.ts';
import type { Delegate, FromWorker, HandCrop, ToWorker, TrackCommand } from './messages.ts';
import modelAssetPath from './models/efficientdet_lite0.tflite?url';
import handModelPath from './models/gesture_recognizer.task?url';
import { Mosse, type Gray, type PixelBox } from './mosse.ts';
import type { Seen } from './tracker.ts';

interface Scope {
  location: Location;
  fetch: typeof fetch;
  onmessage: ((e: MessageEvent<ToWorker>) => void) | null;
  postMessage(message: FromWorker, transfer?: Transferable[]): void;
}

const scope = self as unknown as Scope & { ModuleFactory?: unknown };

// O MediaPipe manda métricas de uso para o Google. Aqui nada sai do app:
// só passam pedidos ao próprio app (wasm e modelo).
const ownFetch = scope.fetch.bind(scope);
scope.fetch = (input, init) => {
  const url = new URL(input instanceof Request ? input.url : String(input), scope.location.href);
  if (/^(https?|wss?):$/.test(url.protocol) && url.origin !== scope.location.origin) {
    return Promise.reject(new TypeError(`acesso à rede bloqueado: ${url.origin}`));
  }
  return ownFetch(input, init);
};

let detector: ObjectDetector | null = null;
let recognizer: GestureRecognizer | null = null;
/** Os gestos foram pedidos (o reconhecedor pode ainda estar carregando). */
let handsWanted = false;
let handsLoading: Promise<void> | null = null;
let delegate: Delegate = 'GPU';
/** Durações das primeiras detecções na GPU (a primeira, mais lenta, fica de fora). */
let gpuTimes: number[] = [];
let tracker: Mosse | null = null;
let canvas: OffscreenCanvas | null = null;
let gray: Gray | null = null;

// Sem placa de vídeo de verdade, o navegador emula a GPU e a CPU sai bem mais rápida.
const GPU_PROBE = 5;
const SLOW_GPU_MS = 80;

let creating: Promise<unknown> = Promise.resolve();

/**
 * Cria uma tarefa do MediaPipe por vez. O carregador do wasm só registra a
 * fábrica na primeira importação, e cada tarefa nova a consome: duas ao mesmo
 * tempo (detector e mãos na troca para a CPU) disputariam a mesma.
 */
function serial<T>(make: (files: { wasmLoaderPath: string; wasmBinaryPath: string }) => Promise<T>): Promise<T> {
  const next = creating.then(async () => {
    const loader = (await import(/* @vite-ignore */ wasmLoaderPath)) as { default: unknown };
    scope.ModuleFactory ??= loader.default;
    return make({ wasmLoaderPath, wasmBinaryPath });
  });
  creating = next.catch(() => {});
  return next;
}

function create(d: Delegate): Promise<ObjectDetector> {
  return serial((files) =>
    ObjectDetector.createFromOptions(files, {
      baseOptions: { modelAssetPath, delegate: d },
      canvas: d === 'GPU' ? new OffscreenCanvas(1, 1) : undefined,
      runningMode: 'VIDEO',
      scoreThreshold: 0.35,
      maxResults: 10,
      categoryAllowlist: Object.keys(FOLLOWABLE),
    }),
  );
}

function createHands(d: Delegate): Promise<GestureRecognizer> {
  return serial((files) =>
    GestureRecognizer.createFromOptions(files, {
      baseOptions: { modelAssetPath: handModelPath, delegate: d },
      canvas: d === 'GPU' ? new OffscreenCanvas(1, 1) : undefined,
      runningMode: 'VIDEO',
      numHands: 1,
    }),
  );
}

/** Carrega o reconhecedor no mesmo processador do detector (com a CPU de reserva). */
async function loadHands(): Promise<void> {
  if (recognizer) return;
  let d = delegate;
  let r: GestureRecognizer;
  try {
    r = await createHands(d);
  } catch (e) {
    if (d === 'CPU') throw e;
    r = await createHands((d = 'CPU'));
  }
  // O detector trocou para a CPU enquanto carregava (GPU lenta): o reconhecedor vai junto.
  if (d !== delegate) {
    r.close();
    r = await createHands(delegate);
  }
  recognizer = r;
}

async function startHands(): Promise<void> {
  handsWanted = true;
  try {
    handsLoading ??= loadHands().finally(() => (handsLoading = null));
    await handsLoading;
    // Desligado enquanto carregava.
    if (!handsWanted) return stopHands();
    scope.postMessage({ type: 'hands', ok: true });
  } catch (e) {
    scope.postMessage({ type: 'hands', ok: false, message: errorText(e) });
  }
}

function stopHands(): void {
  handsWanted = false;
  recognizer?.close();
  recognizer = null;
}

async function init(): Promise<void> {
  try {
    detector = await create('GPU');
    delegate = 'GPU';
  } catch (e) {
    console.warn('IA: GPU indisponível, usando a CPU', e);
    detector = await create('CPU');
    delegate = 'CPU';
  }
}

function detect(bitmap: ImageBitmap, ts: number): Detection[] {
  const { width, height } = bitmap;
  return detector!.detectForVideo(bitmap, ts).detections.flatMap((d) => {
    const b = d.boundingBox;
    const c = d.categories[0];
    if (!b || !c) return [];
    return [{ x: b.originX / width, y: b.originY / height, w: b.width / width, h: b.height / height, category: c.categoryName, score: c.score }];
  });
}

/** O quadro em tons de cinza, para o rastreador visual. */
function toGray(bitmap: ImageBitmap): Gray {
  const { width, height } = bitmap;
  if (!canvas || canvas.width !== width || canvas.height !== height) {
    canvas = new OffscreenCanvas(width, height);
    gray = { data: new Float32Array(width * height), width, height };
  }
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  ctx.drawImage(bitmap, 0, 0);
  const rgba = ctx.getImageData(0, 0, width, height).data;
  const out = gray!.data;
  for (let i = 0, p = 0; i < out.length; i++, p += 4) out[i] = 0.299 * rgba[p] + 0.587 * rgba[p + 1] + 0.114 * rgba[p + 2];
  return gray!;
}

function toPixels(b: Box, img: Gray): PixelBox {
  return { cx: (b.x + b.w / 2) * img.width, cy: (b.y + b.h / 2) * img.height, w: b.w * img.width, h: b.h * img.height };
}

function toBox(p: PixelBox, img: Gray): Box {
  return { x: (p.cx - p.w / 2) / img.width, y: (p.cy - p.h / 2) / img.height, w: p.w / img.width, h: p.h / img.height };
}

/** Gesto e centro da palma no recorte, convertidos para o quadro inteiro. */
function recognize(crop: HandCrop, ts: number): HandSeen {
  const r = recognizer!.recognizeForVideo(crop.bitmap, ts);
  const points = r.landmarks[0];
  if (!points) return { gesture: 'none', score: 0, palm: null };
  const g = r.gestures[0]?.[0];
  // Centro da palma: o punho e a base dos dedos.
  const ids = [0, 5, 9, 13, 17];
  const px = ids.reduce((sum, i) => sum + points[i].x, 0) / ids.length;
  const py = ids.reduce((sum, i) => sum + points[i].y, 0) / ids.length;
  return {
    gesture: toGesture(g?.categoryName, g?.score ?? 0),
    score: g?.score ?? 0,
    palm: { x: crop.box.x + px * crop.box.w, y: crop.box.y + py * crop.box.h },
  };
}

function hand(crop: HandCrop | null, ts: number): HandSeen | null {
  if (!crop) return null;
  try {
    return recognizer ? recognize(crop, ts) : null;
  } catch (e) {
    stopHands();
    scope.postMessage({ type: 'hands', ok: false, message: errorText(e) });
    return null;
  } finally {
    crop.bitmap.close();
  }
}

function track(cmd: TrackCommand | null, bitmap: ImageBitmap): Seen | null {
  if (!cmd) {
    tracker = null;
    return null;
  }
  if (!cmd.init && !tracker) return null;
  const img = toGray(bitmap);
  if (cmd.init) {
    tracker = new Mosse(img, toPixels(cmd.init, img));
    return { box: cmd.init, psr: 99 };
  }
  const r = tracker!.update(img);
  return { box: toBox(r.box, img), psr: r.psr };
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** A GPU falhou ou é lenta demais: refaz o detector na CPU. */
async function switchToCpu(reason: string): Promise<void> {
  console.warn(`IA: usando a CPU (${reason})`);
  // Já vale para um reconhecedor que esteja carregando agora.
  delegate = 'CPU';
  const old = detector;
  detector = null;
  old?.close();
  detector = await create('CPU');
  if (recognizer) {
    recognizer.close();
    recognizer = null;
    const r = await createHands('CPU');
    if (handsWanted && !recognizer) recognizer = r;
    else r.close();
  }
}

scope.onmessage = async ({ data: msg }) => {
  if (msg.type === 'init') {
    try {
      await init();
      scope.postMessage({ type: 'ready', delegate });
    } catch (e) {
      scope.postMessage({ type: 'error', message: errorText(e) });
    }
    return;
  }
  if (msg.type === 'hands') return msg.on ? startHands() : stopHands();
  const started = performance.now();
  const seenHand = hand(msg.hand, msg.ts);
  let detections: Detection[] | null = null;
  let seen: Seen | null = null;
  let detectMs = 0;
  try {
    // O rastreador vem primeiro: é ele que mantém a resposta rápida.
    seen = track(msg.track, msg.bitmap);
    if (msg.detect && detector) {
      const t0 = performance.now();
      detections = detect(msg.bitmap, msg.ts);
      detectMs = performance.now() - t0;
    }
  } catch (e) {
    msg.bitmap.close();
    if (delegate === 'CPU' || !detector) return scope.postMessage({ type: 'error', message: errorText(e) });
    try {
      await switchToCpu(`falha na GPU: ${errorText(e)}`);
      scope.postMessage({ type: 'ready', delegate });
    } catch (err) {
      scope.postMessage({ type: 'error', message: errorText(err) });
    }
    return;
  }
  msg.bitmap.close();
  if (detections && delegate === 'GPU' && gpuTimes.length <= GPU_PROBE) {
    gpuTimes.push(detectMs);
    const typical = median(gpuTimes.slice(1));
    if (gpuTimes.length > GPU_PROBE && typical > SLOW_GPU_MS) {
      try {
        await switchToCpu(`GPU lenta: ${Math.round(typical)} ms por detecção`);
        scope.postMessage({ type: 'ready', delegate });
      } catch (e) {
        return scope.postMessage({ type: 'error', message: errorText(e) });
      }
    }
  }
  scope.postMessage({ type: 'result', ts: msg.ts, detections, track: seen, hand: seenHand, ms: performance.now() - started });
};
