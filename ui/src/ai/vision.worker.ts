// Worker da IA: roda o detector de pose do MediaPipe fora da thread da interface.
// Tudo vem de dentro do app (wasm e modelo), sem internet.

import { PoseLandmarker } from '@mediapipe/tasks-vision';
import wasmLoaderPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.js?url';
import wasmBinaryPath from '@mediapipe/tasks-vision/vision_wasm_module_internal.wasm?url';

import type { Delegate, FromWorker, ToWorker } from './messages.ts';
import modelAssetPath from './models/pose_landmarker_lite.task?url';
import { POINTS } from './pose.ts';

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

let landmarker: PoseLandmarker | null = null;
let delegate: Delegate = 'GPU';
/** Durações das primeiras análises na GPU (a primeira, mais lenta, fica de fora). */
let gpuTimes: number[] = [];

// Sem placa de vídeo de verdade, o navegador emula a GPU e a CPU sai bem mais rápida.
const GPU_PROBE = 5;
const SLOW_GPU_MS = 80;

async function create(d: Delegate): Promise<PoseLandmarker> {
  // O carregador do wasm só registra a fábrica na primeira importação; numa
  // segunda tentativa (GPU → CPU) ela precisa ser registrada de novo.
  const loader = (await import(/* @vite-ignore */ wasmLoaderPath)) as { default: unknown };
  scope.ModuleFactory ??= loader.default;
  return PoseLandmarker.createFromOptions(
    { wasmLoaderPath, wasmBinaryPath },
    {
      baseOptions: { modelAssetPath, delegate: d },
      canvas: d === 'GPU' ? new OffscreenCanvas(1, 1) : undefined,
      runningMode: 'VIDEO',
      numPoses: 2,
      minPoseDetectionConfidence: 0.5,
      minPosePresenceConfidence: 0.5,
      minTrackingConfidence: 0.5,
    },
  );
}

async function init(): Promise<void> {
  try {
    landmarker = await create('GPU');
    delegate = 'GPU';
  } catch (e) {
    console.warn('IA: GPU indisponível, usando a CPU', e);
    landmarker = await create('CPU');
    delegate = 'CPU';
  }
}

function detect(bitmap: ImageBitmap, ts: number): Float32Array[] {
  const result = landmarker!.detectForVideo(bitmap, ts);
  return result.landmarks.map((points) => {
    const pose = new Float32Array(POINTS * 3);
    points.slice(0, POINTS).forEach((p, i) => {
      pose[i * 3] = p.x;
      pose[i * 3 + 1] = p.y;
      pose[i * 3 + 2] = p.visibility ?? 0;
    });
    return pose;
  });
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
  const old = landmarker;
  landmarker = null;
  old?.close();
  landmarker = await create('CPU');
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
  if (!landmarker) {
    // Trocando de GPU para CPU: devolve um resultado vazio para não travar a fila.
    msg.bitmap.close();
    return scope.postMessage({ type: 'result', poses: [], ms: 0 });
  }
  const started = performance.now();
  let poses: Float32Array[];
  try {
    poses = detect(msg.bitmap, msg.ts);
  } catch (e) {
    if (delegate === 'CPU') return scope.postMessage({ type: 'error', message: errorText(e) });
    try {
      await switchToCpu(`falha na GPU: ${errorText(e)}`);
      scope.postMessage({ type: 'ready', delegate });
    } catch (err) {
      scope.postMessage({ type: 'error', message: errorText(err) });
    }
    return;
  } finally {
    msg.bitmap.close();
  }
  const ms = performance.now() - started;
  if (delegate === 'GPU' && gpuTimes.length <= GPU_PROBE) {
    gpuTimes.push(ms);
    const typical = median(gpuTimes.slice(1));
    if (gpuTimes.length > GPU_PROBE && typical > SLOW_GPU_MS) {
      try {
        await switchToCpu(`GPU lenta: ${Math.round(typical)} ms por análise`);
        scope.postMessage({ type: 'ready', delegate });
      } catch (e) {
        return scope.postMessage({ type: 'error', message: errorText(e) });
      }
    }
  }
  scope.postMessage({ type: 'result', poses, ms }, poses.map((p) => p.buffer));
};
