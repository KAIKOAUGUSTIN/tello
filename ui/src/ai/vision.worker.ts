// Worker da IA: roda o detector de objetos do MediaPipe e o rastreador visual
// fora da thread da interface. Tudo vem de dentro do app (wasm e modelo), sem
// internet.

import { ObjectDetector } from '@mediapipe/tasks-vision';
import wasmLoaderPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.js?url';
import wasmBinaryPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.wasm?url';

import { FOLLOWABLE, type Box, type Detection } from './boxes.ts';
import type { Delegate, FromWorker, ToWorker, TrackCommand } from './messages.ts';
import modelAssetPath from './models/efficientdet_lite0.tflite?url';
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
let delegate: Delegate = 'GPU';
/** Durações das primeiras detecções na GPU (a primeira, mais lenta, fica de fora). */
let gpuTimes: number[] = [];
let tracker: Mosse | null = null;
let canvas: OffscreenCanvas | null = null;
let gray: Gray | null = null;

// Sem placa de vídeo de verdade, o navegador emula a GPU e a CPU sai bem mais rápida.
const GPU_PROBE = 5;
const SLOW_GPU_MS = 80;

async function create(d: Delegate): Promise<ObjectDetector> {
  // O carregador do wasm só registra a fábrica na primeira importação; numa
  // segunda tentativa (GPU → CPU) ela precisa ser registrada de novo.
  const loader = (await import(/* @vite-ignore */ wasmLoaderPath)) as { default: unknown };
  scope.ModuleFactory ??= loader.default;
  return ObjectDetector.createFromOptions(
    { wasmLoaderPath, wasmBinaryPath },
    {
      baseOptions: { modelAssetPath, delegate: d },
      canvas: d === 'GPU' ? new OffscreenCanvas(1, 1) : undefined,
      runningMode: 'VIDEO',
      scoreThreshold: 0.35,
      maxResults: 10,
      categoryAllowlist: Object.keys(FOLLOWABLE),
    },
  );
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
  const old = detector;
  detector = null;
  old?.close();
  detector = await create('CPU');
  delegate = 'CPU';
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
  const started = performance.now();
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
  scope.postMessage({ type: 'result', ts: msg.ts, detections, track: seen, ms: performance.now() - started });
};
