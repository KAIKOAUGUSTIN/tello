// Rastreador visual MOSSE (Bolme et al., 2010): um filtro de correlação que
// aprende a aparência do alvo e o reencontra no quadro seguinte em poucos
// milissegundos. Serve para qualquer coisa marcada na tela, mesmo o que o
// detector não conhece. Trabalha numa imagem em tons de cinza. Módulo puro.

export interface Gray {
  data: Float32Array;
  width: number;
  height: number;
}

/** Caixa em pixels da imagem, pelo centro. */
export interface PixelBox {
  cx: number;
  cy: number;
  w: number;
  h: number;
}

export const MOSSE = {
  /** Quanto o filtro aprende a cada quadro. */
  learningRate: 0.125,
  /** Largura do pico desejado, em pixels da janela. */
  sigma: 2,
  /** Janela de busca em relação à caixa. */
  padding: 2,
  /** Escalas testadas a cada quadro. */
  scales: [0.96, 1, 1.04],
  /** Penalidade para trocar de escala (evita a caixa "respirar" sozinha). */
  scalePenalty: 0.97,
  /** Abaixo disso o alvo não foi encontrado (e o filtro não aprende). */
  lostPsr: 7,
};

/** Lado da grade da FFT. */
export const N = 64;
const MIN_SIDE = 8;
const SIDELOBE = 5;

// Tabelas da FFT de N pontos.
const REV = new Uint16Array(N);
for (let i = 0; i < N; i++) {
  let r = 0;
  for (let b = 1, v = i; b < N; b <<= 1, v >>= 1) r = (r << 1) | (v & 1);
  REV[i] = r;
}
const COS = new Float64Array(N / 2);
const SIN = new Float64Array(N / 2);
for (let t = 0; t < N / 2; t++) {
  COS[t] = Math.cos((2 * Math.PI * t) / N);
  SIN[t] = Math.sin((2 * Math.PI * t) / N);
}

function fft1d(re: Float64Array, im: Float64Array, off: number, stride: number, inverse: boolean): void {
  for (let i = 0; i < N; i++) {
    const j = REV[i];
    if (j > i) {
      const a = off + i * stride;
      const b = off + j * stride;
      [re[a], re[b]] = [re[b], re[a]];
      [im[a], im[b]] = [im[b], im[a]];
    }
  }
  for (let size = 2; size <= N; size <<= 1) {
    const half = size >> 1;
    const step = N / size;
    for (let start = 0; start < N; start += size) {
      for (let k = 0; k < half; k++) {
        const wr = COS[k * step];
        const wi = inverse ? SIN[k * step] : -SIN[k * step];
        const a = off + (start + k) * stride;
        const b = off + (start + k + half) * stride;
        const tr = wr * re[b] - wi * im[b];
        const ti = wr * im[b] + wi * re[b];
        re[b] = re[a] - tr;
        im[b] = im[a] - ti;
        re[a] += tr;
        im[a] += ti;
      }
    }
  }
}

export function fft2d(re: Float64Array, im: Float64Array, inverse = false): void {
  for (let r = 0; r < N; r++) fft1d(re, im, r * N, 1, inverse);
  for (let c = 0; c < N; c++) fft1d(re, im, c, N, inverse);
  if (inverse) {
    const k = 1 / (N * N);
    for (let i = 0; i < N * N; i++) {
      re[i] *= k;
      im[i] *= k;
    }
  }
}

// Janela de Hann (bordas suaves) e o pico gaussiano desejado, no domínio da frequência.
export const HANN = new Float64Array(N * N);
const G_RE = new Float64Array(N * N);
const G_IM = new Float64Array(N * N);
{
  const s = MOSSE.sigma;
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      HANN[i * N + j] = Math.sin((Math.PI * (i + 0.5)) / N) ** 2 * Math.sin((Math.PI * (j + 0.5)) / N) ** 2;
      G_RE[i * N + j] = Math.exp(-((i - N / 2) ** 2 + (j - N / 2) ** 2) / (2 * s * s));
    }
  }
  fft2d(G_RE, G_IM);
}

function sample(img: Gray, x: number, y: number): number {
  const { data, width, height } = img;
  const fx = Math.min(width - 1, Math.max(0, x));
  const fy = Math.min(height - 1, Math.max(0, y));
  const x0 = Math.min(width - 2, Math.floor(fx));
  const y0 = Math.min(height - 2, Math.floor(fy));
  const ax = fx - x0;
  const ay = fy - y0;
  const i = y0 * width + x0;
  const top = data[i] + ax * (data[i + 1] - data[i]);
  const bottom = data[i + width] + ax * (data[i + width + 1] - data[i + width]);
  return top + ay * (bottom - top);
}

/**
 * Recorta a janela em volta da caixa numa grade N×N, já pronta para a FFT:
 * logaritmo, média zero, desvio 1 e janela de Hann. `angle` e `zoom` deformam
 * um pouco o recorte (usado só para treinar o filtro inicial).
 */
function patch(img: Gray, box: PixelBox, scale: number, re: Float64Array, im: Float64Array, angle = 0, zoom = 1): void {
  const ww = Math.max(MIN_SIDE, box.w) * MOSSE.padding * scale;
  const wh = Math.max(MIN_SIDE, box.h) * MOSSE.padding * scale;
  const c = Math.cos(angle) * zoom;
  const s = Math.sin(angle) * zoom;
  let sum = 0;
  for (let i = 0; i < N; i++) {
    const v = (i + 0.5) / N - 0.5;
    for (let j = 0; j < N; j++) {
      const u = (j + 0.5) / N - 0.5;
      const x = box.cx + (c * u - s * v) * ww;
      const y = box.cy + (s * u + c * v) * wh;
      const p = Math.log1p(Math.max(0, sample(img, x, y)));
      re[i * N + j] = p;
      sum += p;
    }
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

/** Resultado de uma busca: altura do pico, onde ele ficou (na grade) e a nitidez. */
interface Found {
  peak: number;
  dx: number;
  dy: number;
  psr: number;
}

/** Gerador pseudoaleatório fixo: o treino inicial sai sempre igual. */
function lcg(seed: number): () => number {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 2 ** 32);
}

export class Mosse {
  box: PixelBox;
  /** Nitidez do pico da última busca (peak-to-sidelobe ratio). */
  psr = 0;
  private readonly aRe = new Float64Array(N * N);
  private readonly aIm = new Float64Array(N * N);
  private readonly b = new Float64Array(N * N);
  private readonly re = new Float64Array(N * N);
  private readonly im = new Float64Array(N * N);
  private readonly resp = new Float64Array(N * N);

  constructor(img: Gray, box: PixelBox) {
    this.box = { ...box, w: Math.max(MIN_SIDE, box.w), h: Math.max(MIN_SIDE, box.h) };
    // Treina com o recorte original e algumas versões levemente giradas e
    // ampliadas, para o filtro já começar tolerante.
    const rand = lcg(0x6d6f7373);
    for (let k = 0; k < 8; k++) {
      const angle = k === 0 ? 0 : (rand() - 0.5) * 0.25;
      const zoom = k === 0 ? 1 : 0.94 + rand() * 0.12;
      patch(img, this.box, 1, this.re, this.im, angle, zoom);
      fft2d(this.re, this.im);
      this.accumulate(1);
    }
    this.psr = Infinity;
  }

  /**
   * Procura o alvo na imagem nova e devolve a caixa e a confiança. `shift`
   * (pixels) é quanto a imagem andou com o giro da câmera desde o último
   * quadro: a busca começa ali, e também no lugar antigo, caso a medida erre.
   */
  update(img: Gray, shift: { dx: number; dy: number } = { dx: 0, dy: 0 }): { box: PixelBox; psr: number } {
    const moved: PixelBox = {
      ...this.box,
      cx: Math.min(img.width, Math.max(0, this.box.cx + shift.dx)),
      cy: Math.min(img.height, Math.max(0, this.box.cy + shift.dy)),
    };
    const starts = Math.hypot(shift.dx, shift.dy) >= 1 ? [moved, this.box] : [moved];
    let from = moved;
    let base: Found | null = null;
    for (const start of starts) {
      const r = this.search(img, start, 1);
      if (!base || r.psr > base.psr) [from, base] = [start, r];
    }
    let best = { score: base!.peak, scale: 1, dx: base!.dx, dy: base!.dy, psr: base!.psr };
    for (const scale of MOSSE.scales) {
      if (scale === 1) continue;
      const r = this.search(img, from, scale);
      const score = r.peak * MOSSE.scalePenalty;
      if (score > best.score) best = { score, scale, dx: r.dx, dy: r.dy, psr: r.psr };
    }
    const ww = this.box.w * MOSSE.padding * best.scale;
    const wh = this.box.h * MOSSE.padding * best.scale;
    const next: PixelBox = {
      cx: Math.min(img.width, Math.max(0, from.cx + (best.dx * ww) / N)),
      cy: Math.min(img.height, Math.max(0, from.cy + (best.dy * wh) / N)),
      w: Math.min(img.width, Math.max(MIN_SIDE, this.box.w * best.scale)),
      h: Math.min(img.height, Math.max(MIN_SIDE, this.box.h * best.scale)),
    };
    this.psr = best.psr;
    if (best.psr >= MOSSE.lostPsr) {
      // Achou: anda até lá e aprende a aparência nova.
      this.box = next;
      patch(img, this.box, 1, this.re, this.im);
      fft2d(this.re, this.im);
      this.accumulate(MOSSE.learningRate);
    } else {
      // Perdido: não aprende (para não decorar o que está na frente), mas anda
      // com a imagem, para a busca seguinte começar onde o alvo deve estar.
      this.box = moved;
    }
    return { box: { ...this.box }, psr: best.psr };
  }

  private accumulate(rate: number): void {
    const keep = rate === 1 ? 1 : 1 - rate;
    for (let k = 0; k < N * N; k++) {
      const fr = this.re[k];
      const fi = this.im[k];
      // A = Σ G·F̄ e B = Σ F·F̄ (com média móvel depois do treino inicial).
      const ar = G_RE[k] * fr + G_IM[k] * fi;
      const ai = G_IM[k] * fr - G_RE[k] * fi;
      this.aRe[k] = this.aRe[k] * keep + ar * rate;
      this.aIm[k] = this.aIm[k] * keep + ai * rate;
      this.b[k] = this.b[k] * keep + (fr * fr + fi * fi) * rate;
    }
  }

  private search(img: Gray, center: PixelBox, scale: number): Found {
    const { re, im, resp } = this;
    patch(img, center, scale, re, im);
    fft2d(re, im);
    // Resposta = F · A / B, de volta ao espaço da imagem.
    const lambda = 1e-2;
    for (let k = 0; k < N * N; k++) {
      const d = 1 / (this.b[k] + lambda);
      const r = (re[k] * this.aRe[k] - im[k] * this.aIm[k]) * d;
      const i = (re[k] * this.aIm[k] + im[k] * this.aRe[k]) * d;
      re[k] = r;
      im[k] = i;
    }
    fft2d(re, im, true);
    let peak = -Infinity;
    let pi = 0;
    let pj = 0;
    for (let k = 0; k < N * N; k++) {
      resp[k] = re[k];
      if (re[k] > peak) [peak, pi, pj] = [re[k], (k / N) | 0, k % N];
    }
    // Nitidez do pico: o quanto ele se destaca do resto da resposta.
    let sum = 0;
    let sq = 0;
    let count = 0;
    for (let i = 0; i < N; i++) {
      for (let j = 0; j < N; j++) {
        if (Math.abs(i - pi) <= SIDELOBE && Math.abs(j - pj) <= SIDELOBE) continue;
        const v = resp[i * N + j];
        sum += v;
        sq += v * v;
        count++;
      }
    }
    const mean = sum / count;
    const std = Math.sqrt(Math.max(1e-12, sq / count - mean * mean));
    // Posição com fração de pixel (parábola nos vizinhos do pico).
    const at = (i: number, j: number) => resp[((i + N) % N) * N + ((j + N) % N)];
    const subpixel = (l: number, c: number, r: number) => {
      const den = l - 2 * c + r;
      return den < 0 ? Math.max(-0.5, Math.min(0.5, (0.5 * (l - r)) / den)) : 0;
    };
    const dy = pi - N / 2 + subpixel(at(pi - 1, pj), peak, at(pi + 1, pj));
    const dx = pj - N / 2 + subpixel(at(pi, pj - 1), peak, at(pi, pj + 1));
    return { peak, dx, dy, psr: (peak - mean) / std };
  }
}
