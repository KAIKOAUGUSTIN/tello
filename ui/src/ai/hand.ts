// Gestos da mão, sem dependências (testado no Node): filtro no tempo, guia pela
// palma, contagem da foto pelo V, recorte onde procurar a mão e quem comanda.
// O reconhecimento em si (pontos da mão e gesto) é do MediaPipe, no worker.

import { iou, type Box, type Detection } from './boxes.ts';

export type Gesture = 'none' | 'victory' | 'palm' | 'fist';

/** O que o reconhecedor achou num recorte, já em fração do quadro inteiro. */
export interface HandSeen {
  gesture: Gesture;
  score: number;
  /** Centro da palma, ou `null` sem mão no recorte. */
  palm: Point | null;
}

export interface Point {
  x: number;
  y: number;
}

export const HAND = {
  /** Recorte: a parte de cima da pessoa, com folga, quadrado (em alturas do quadro). */
  cropTop: 0.65,
  cropPad: 1.3,
  cropMin: 0.15,
  /** Abaixo disso o reconhecedor não tem certeza do gesto. */
  minScore: 0.5,
  /** Filtro: o gesto vale com essa parte dos quadros da janela, e um mínimo de quadros. */
  windowMs: 500,
  agree: 0.7,
  minSamples: 3,
  /** Sem o gesto por esse tempo, ele solta. */
  releaseMs: 300,
  /** Guia, em alturas da pessoa (a mão a 2 m ou a 4 m se move igual). */
  deadband: 0.05,
  throttleGain: 2,
  throttleMax: 0.3,
  rollGain: 1.5,
  rollMax: 0.25,
  /** Abaixo disso (cm do chão) a mão não faz descer. */
  minHeightCm: 50,
  /** V parado por esse tempo começa a contagem da foto. */
  victoryHoldMs: 500,
  countdownMs: 3000,
  /** Entre uma foto e a próxima contagem. */
  cooldownMs: 5000,
};

const NAMES: Record<string, Gesture> = { Victory: 'victory', Open_Palm: 'palm', Closed_Fist: 'fist' };

/** Gesto do MediaPipe (`Victory`, `Open_Palm`...) para os que o app usa. */
export function toGesture(name: string | undefined, score: number): Gesture {
  return name && score >= HAND.minScore ? (NAMES[name] ?? 'none') : 'none';
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

function deadband(v: number, band: number): number {
  return Math.abs(v) <= band ? 0 : v - Math.sign(v) * band;
}

/** Só aceita um gesto que se repete por um tempo e o solta quando some, para um quadro errado não mandar no drone. */
export class GestureFilter {
  /** Gesto valendo agora, e desde quando. */
  current: Gesture = 'none';
  since = 0;
  private samples: { t: number; g: Gesture }[] = [];
  private lastSeen = 0;

  update(g: Gesture, t: number): Gesture {
    this.samples.push({ t, g });
    while (t - this.samples[0].t > HAND.windowMs) this.samples.shift();
    if (g !== 'none' && g === this.current) this.lastSeen = t;

    const counts = new Map<Gesture, number>();
    for (const s of this.samples) if (s.g !== 'none') counts.set(s.g, (counts.get(s.g) ?? 0) + 1);
    let best: Gesture = 'none';
    let n = 0;
    for (const [k, c] of counts) {
      if (c > n) [best, n] = [k, c];
    }
    const agreed = this.samples.length >= HAND.minSamples && n / this.samples.length >= HAND.agree;
    if (agreed && best !== this.current) {
      this.current = best;
      this.since = this.lastSeen = t;
    } else if (this.current !== 'none' && t - this.lastSeen > HAND.releaseMs) {
      this.current = 'none';
      this.since = t;
    }
    return this.current;
  }
}

/**
 * Palma em relação ao corpo, em alturas da pessoa. Quando o drone sobe, gira ou
 * inclina, corpo e mão andam juntos na imagem e a diferença não muda.
 */
export function palmOffset(palm: Point, body: Box, aspect: number): Point {
  return { x: ((palm.x - (body.x + body.w / 2)) * aspect) / body.h, y: (palm.y - (body.y + body.h / 2)) / body.h };
}

/**
 * Palma aberta guia: o lugar da mão quando a guia começa é o centro. Mão acima
 * dele, o drone sobe; abaixo, desce; para um lado, ele vai para esse lado (o
 * da pessoa, não o da imagem). De volta ao centro, ele para.
 */
export class PalmGuide {
  private readonly ref: Point;

  constructor(palm: Point, body: Box, aspect: number) {
    this.ref = palmOffset(palm, body, aspect);
  }

  step(palm: Point, body: Box, aspect: number, heightCm: number | null): { roll: number; throttle: number } {
    const p = palmOffset(palm, body, aspect);
    let throttle = clamp(HAND.throttleGain * deadband(this.ref.y - p.y, HAND.deadband), -HAND.throttleMax, HAND.throttleMax);
    // A câmera olha para a pessoa: a mão indo para a direita dela aparece indo
    // para a esquerda da imagem, que é a esquerda do drone, o mesmo lado no mundo.
    const roll = clamp(HAND.rollGain * deadband(p.x - this.ref.x, HAND.deadband), -HAND.rollMax, HAND.rollMax);
    // Perto do chão (ou sem saber a altura), a mão não faz descer.
    if (throttle < 0 && (heightCm === null || heightCm < HAND.minHeightCm)) throttle = 0;
    return { roll, throttle };
  }
}

/** V parado por meio segundo: contagem de 3 s e foto. Depois, o V precisa sumir e voltar. */
export class PhotoTrigger {
  private started: number | null = null;
  private lastShot = -Infinity;
  private armed = true;

  get counting(): boolean {
    return this.started !== null;
  }

  /** A cada gesto estável; `true` quando a contagem começa. */
  see(stable: Gesture, since: number, t: number): boolean {
    if (stable !== 'victory') {
      this.armed = true;
      return false;
    }
    if (this.started !== null || !this.armed) return false;
    if (t - since < HAND.victoryHoldMs || t - this.lastShot < HAND.cooldownMs) return false;
    this.started = t;
    this.armed = false;
    return true;
  }

  /** Segundos que faltam, arredondados para cima, ou `null` sem contagem. */
  remaining(t: number): number | null {
    return this.started === null ? null : Math.max(1, Math.ceil((HAND.countdownMs - (t - this.started)) / 1000));
  }

  /** `true` na hora da foto. */
  tick(t: number): boolean {
    if (this.started === null || t - this.started < HAND.countdownMs) return false;
    this.started = null;
    this.lastShot = t;
    return true;
  }

  cancel(): void {
    this.started = null;
  }
}

/**
 * Onde procurar a mão: a parte de cima da pessoa, com folga, num quadrado (em
 * pixels) dentro do quadro. O recorte vem do quadro em resolução cheia, para a
 * mão a 3 m ainda ter tamanho.
 */
export function handCrop(person: Box, aspect: number): Box {
  const top = person.h * HAND.cropTop;
  const cx = person.x + person.w / 2;
  const cy = person.y + top / 2;
  // Lado em alturas do quadro: largura = aspect.
  const s = Math.max(HAND.cropMin, Math.min(Math.max(person.w * aspect, top) * HAND.cropPad, 1, aspect));
  const w = s / aspect;
  return { x: clamp(cx - w / 2, 0, 1 - w), y: clamp(cy - s / 2, 0, 1 - s), w, h: s };
}

/** Quem comanda com a mão: a mesma pessoa de antes, se ainda estiver lá; senão, a maior perto do centro. */
export function pickCommander(detections: readonly Detection[], prev: Box | null): Detection | null {
  const people = detections.filter((d) => d.category === 'person');
  let best: Detection | null = null;
  let score = -Infinity;
  for (const d of people) {
    const same = prev ? iou(d, prev) : 0;
    const s = same >= 0.3 ? 10 + same : d.h * (1 - Math.abs(d.x + d.w / 2 - 0.5));
    if (s > score) [best, score] = [d, s];
  }
  return best;
}
