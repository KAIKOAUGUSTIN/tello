// Movimento da própria câmera entre dois quadros, medido na imagem inteira por
// correlação de fase. Quando o drone gira (ou inclina), tudo na imagem anda
// junto e bem mais rápido que o alvo, e o rastreador visual, que só procura
// perto de onde ele estava, perde. Sabendo quanto a câmera girou, a busca vai
// para onde o alvo foi parar. A DJI tira isso da IMU e do gimbal; o Tello não
// tem gimbal e a telemetria não chega sincronizada com o vídeo, então a medida
// sai do próprio quadro. Módulo puro.

import type { Box } from './boxes.ts';
import { fft2d, HANN, N, type Gray } from './mosse.ts';

/** Câmera do Tello: 82,6° na diagonal, em 4:3. */
export const CAMERA = { hfovDeg: 70, vfovDeg: 56 };

export const MOTION = {
  /**
   * Abaixo disso a medida não vale (imagem lisa ou cena trocada). Dois quadros
   * sem relação dão 4 a 6; um giro de verdade, mesmo borrado, dá de 18 a 200.
   */
  minPsr: 10,
  /** Maior giro aceito entre dois quadros, em fração do campo de visão. */
  maxShift: 0.25,
  /**
   * Num quadro sem medida (o alvo apareceu de repente, um borrão forte), o
   * giro de há pouco continua valendo por esse tempo: o drone tem inércia.
   */
  holdMs: 300,
};

/**
 * Giro da câmera, em radianos, como a imagem o vê: `pan` > 0, tudo anda para a
 * direita (o drone girou para a esquerda); `tilt` > 0, tudo desce.
 */
export interface Motion {
  pan: number;
  tilt: number;
}

const TAN_H = Math.tan((CAMERA.hfovDeg * Math.PI) / 360);
const TAN_V = Math.tan((CAMERA.vfovDeg * Math.PI) / 360);
const FOV_H = (CAMERA.hfovDeg * Math.PI) / 180;
const FOV_V = (CAMERA.vfovDeg * Math.PI) / 180;

/**
 * Peso de cada frequência: os detalhes mais finos que a grade se embaralham
 * ao reduzir a imagem e pesam menos (σ em ciclos por quadro).
 */
const WEIGHT = new Float64Array(N * N);
{
  const sigma = 10;
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const fy = Math.min(i, N - i);
      const fx = Math.min(j, N - j);
      WEIGHT[i * N + j] = Math.exp(-(fx * fx + fy * fy) / (2 * sigma * sigma));
    }
  }
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

/** Um eixo da câmera pinhole: o ponto (fração do quadro) depois do giro. Na borda ele anda mais que no centro. */
function turn(p: number, angle: number, tanHalf: number): number {
  const a = Math.atan((p - 0.5) * 2 * tanHalf) + angle;
  return 0.5 + Math.tan(clamp(a, -1.4, 1.4)) / (2 * tanHalf);
}

/** Onde um ponto do quadro (fração) vai parar com o giro da câmera. */
export function moveWith(m: Motion, x: number, y: number): { x: number; y: number } {
  return { x: turn(x, m.pan, TAN_H), y: turn(y, m.tilt, TAN_V) };
}

/** A caixa levada pelo giro da câmera (o tamanho não muda). */
export function moveBox(b: Box, m: Motion): Box {
  const c = moveWith(m, b.x + b.w / 2, b.y + b.h / 2);
  return { ...b, x: c.x - b.w / 2, y: c.y - b.h / 2 };
}

export class CameraMotion {
  private readonly cell = new Float64Array(N * N);
  private readonly count = new Float64Array(N * N);
  private readonly re = new Float64Array(N * N);
  private readonly im = new Float64Array(N * N);
  private readonly prevRe = new Float64Array(N * N);
  private readonly prevIm = new Float64Array(N * N);
  private cols: Int32Array = new Int32Array(0);
  private rows: Int32Array = new Int32Array(0);
  private has = false;
  private last = 0;
  /** Último giro medido, por milissegundo, e quando. */
  private rate: Motion | null = null;
  private measuredAt = -Infinity;

  /**
   * Giro desde o quadro anterior (`t` em ms), ou `null` sem medida confiável
   * (no primeiro, por exemplo).
   */
  update(img: Gray, t: number): Motion | null {
    const { re, im, prevRe, prevIm } = this;
    this.shrink(img);
    fft2d(re, im);
    const dt = t - this.last;
    this.last = t;
    if (!this.has) {
      prevRe.set(re);
      prevIm.set(im);
      this.has = true;
      return null;
    }
    // Espectro cruzado normalizado: só a fase fica, e a volta dá um pico no deslocamento.
    for (let k = 0; k < N * N; k++) {
      const ar = re[k];
      const ai = im[k];
      const cr = ar * prevRe[k] + ai * prevIm[k];
      const ci = ai * prevRe[k] - ar * prevIm[k];
      prevRe[k] = ar;
      prevIm[k] = ai;
      const d = WEIGHT[k] / (Math.hypot(cr, ci) + 1e-9);
      re[k] = cr * d;
      im[k] = ci * d;
    }
    fft2d(re, im, true);
    const peak = findPeak(re);
    const dx = peak.dx / N;
    const dy = peak.dy / N;
    if (peak.psr < MOTION.minPsr || Math.abs(dx) > MOTION.maxShift || Math.abs(dy) > MOTION.maxShift) {
      const r = this.rate;
      return r && t - this.measuredAt <= MOTION.holdMs ? { pan: r.pan * dt, tilt: r.tilt * dt } : null;
    }
    // A grade é em ângulos iguais: o deslocamento nela já é o giro.
    const m = { pan: dx * FOV_H, tilt: dy * FOV_V };
    if (dt > 0) this.rate = { pan: m.pan / dt, tilt: m.tilt / dt };
    this.measuredAt = t;
    return m;
  }

  /** Esquece o quadro anterior (o próximo só serve de referência). */
  reset(): void {
    this.has = false;
    this.rate = null;
  }

  /**
   * A imagem inteira numa grade N×N, pronta para a FFT. Cada célula é a média
   * de um mesmo ângulo de visão (sem serrilhado): na lente, um giro anda mais
   * pixels na borda que no centro, mas na grade é um deslocamento só.
   */
  private shrink(img: Gray): void {
    const { data, width, height } = img;
    const { cell, count, re, im } = this;
    if (this.cols.length !== width || this.rows.length !== height) {
      this.cols = cellsOf(width, TAN_H);
      this.rows = cellsOf(height, TAN_V);
    }
    const { cols, rows } = this;
    cell.fill(0);
    count.fill(0);
    for (let y = 0; y < height; y++) {
      const row = rows[y] * N;
      for (let x = 0, i = y * width; x < width; x++, i++) {
        cell[row + cols[x]] += data[i];
        count[row + cols[x]]++;
      }
    }
    let sum = 0;
    for (let k = 0; k < N * N; k++) {
      re[k] = Math.log1p(Math.max(0, cell[k] / Math.max(1, count[k])));
      sum += re[k];
    }
    const mean = sum / (N * N);
    let sq = 0;
    for (let k = 0; k < N * N; k++) sq += (re[k] - mean) ** 2;
    const inv = 1 / (Math.sqrt(sq / (N * N)) + 1e-5);
    for (let k = 0; k < N * N; k++) {
      re[k] = (re[k] - mean) * inv * HANN[k];
      im[k] = 0;
    }
  }
}

/** Em que célula (de ângulo igual) cai cada pixel de um eixo. */
function cellsOf(size: number, tanHalf: number): Int32Array {
  const half = Math.atan(tanHalf);
  const out = new Int32Array(size);
  for (let p = 0; p < size; p++) {
    const a = Math.atan(((p + 0.5) / size - 0.5) * 2 * tanHalf);
    out[p] = Math.min(N - 1, Math.floor(((a / half + 1) / 2) * N));
  }
  return out;
}

const SIDELOBE = 3;

/** Pico da correlação (com volta nas bordas: o índice N-1 é -1) e a nitidez dele. */
function findPeak(resp: Float64Array): { dx: number; dy: number; psr: number } {
  let peak = -Infinity;
  let pi = 0;
  let pj = 0;
  for (let k = 0; k < N * N; k++) if (resp[k] > peak) [peak, pi, pj] = [resp[k], (k / N) | 0, k % N];
  let sum = 0;
  let sq = 0;
  let n = 0;
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const di = Math.min(Math.abs(i - pi), N - Math.abs(i - pi));
      const dj = Math.min(Math.abs(j - pj), N - Math.abs(j - pj));
      if (di <= SIDELOBE && dj <= SIDELOBE) continue;
      const v = resp[i * N + j];
      sum += v;
      sq += v * v;
      n++;
    }
  }
  const mean = sum / n;
  const std = Math.sqrt(Math.max(1e-12, sq / n - mean * mean));
  const at = (i: number, j: number) => resp[((i + N) % N) * N + ((j + N) % N)];
  const subpixel = (l: number, c: number, r: number) => {
    const den = l - 2 * c + r;
    return den < 0 ? clamp((0.5 * (l - r)) / den, -0.5, 0.5) : 0;
  };
  const wrap = (v: number) => (v >= N / 2 ? v - N : v);
  return {
    dx: wrap(pj) + subpixel(at(pi, pj - 1), peak, at(pi, pj + 1)),
    dy: wrap(pi) + subpixel(at(pi - 1, pj), peak, at(pi + 1, pj)),
    psr: (peak - mean) / std,
  };
}
