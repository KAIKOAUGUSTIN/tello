import type { VideoPacket } from '../lib/types';
import { codecFromSps, findSps } from './h264';

export interface PlayerStats {
  fps: number;
  width: number;
  height: number;
  codec: string | null;
  decoded: number;
  dropped: number;
  errors: number;
  /** O sistema não decodifica este H.264 (ex.: navegador sem codecs proprietários). */
  unsupported: boolean;
}

/** Fila máxima no decodificador antes de descartar frames para não acumular atraso. */
const MAX_QUEUE = 3;

/**
 * Decodifica o H.264 do drone com WebCodecs (aceleração de hardware quando
 * disponível) e desenha num canvas.
 */
export class H264Player {
  private decoder: VideoDecoder | null = null;
  private codec: string | null = null;
  private waitingKey = true;
  private configuring = false;
  private pending: VideoPacket | null = null;
  private ctx: CanvasRenderingContext2D | null;
  private frameTimes: number[] = [];
  private lastStats = 0;
  readonly stats: PlayerStats = {
    fps: 0,
    width: 0,
    height: 0,
    codec: null,
    decoded: 0,
    dropped: 0,
    errors: 0,
    unsupported: false,
  };

  constructor(
    private canvas: HTMLCanvasElement,
    private onStats: (s: PlayerStats) => void = () => {},
  ) {
    this.ctx = canvas.getContext('2d', { alpha: false, desynchronized: true });
  }

  static supported(): boolean {
    return typeof VideoDecoder !== 'undefined';
  }

  push(p: VideoPacket): void {
    if (!H264Player.supported() || this.stats.unsupported) return;
    if (this.configuring) {
      if (p.key) this.pending = p;
      return;
    }
    if (p.hasSps) {
      const sps = findSps(p.data);
      if (sps) {
        const codec = codecFromSps(sps);
        if (codec !== this.codec || !this.decoder || this.decoder.state === 'closed') {
          // Configura e decodifica este mesmo keyframe quando estiver pronto.
          this.pending = p;
          void this.configure(codec);
          return;
        }
      }
    }
    const dec = this.decoder;
    if (!dec || dec.state !== 'configured') return;
    if (this.waitingKey && !p.key) {
      this.stats.dropped++;
      return;
    }
    if (!p.key && dec.decodeQueueSize > MAX_QUEUE) {
      // O decodificador ficou para trás: pula até o próximo keyframe.
      this.stats.dropped++;
      this.waitingKey = true;
      return;
    }
    try {
      dec.decode(new EncodedVideoChunk({ type: p.key ? 'key' : 'delta', timestamp: p.seq * 33_333, data: p.data }));
      this.waitingKey = false;
    } catch (e) {
      this.fail(e);
    }
  }

  /** Esquece o estado atual (ex.: nova conexão). */
  reset(): void {
    this.closeDecoder();
    this.pending = null;
    this.codec = null;
    this.waitingKey = true;
    this.frameTimes = [];
    this.stats.fps = 0;
    this.publish(true);
  }

  close(): void {
    this.closeDecoder();
  }

  async snapshotPng(): Promise<Uint8Array> {
    const blob = await new Promise<Blob | null>((r) => this.canvas.toBlob(r, 'image/png'));
    if (!blob) throw new Error('sem imagem para capturar');
    return new Uint8Array(await blob.arrayBuffer());
  }

  private async configure(codec: string): Promise<void> {
    this.configuring = true;
    this.closeDecoder();
    // Sem `description`: o decodificador aceita Annex B com SPS/PPS no fluxo.
    const config: VideoDecoderConfig = { codec, optimizeForLatency: true, hardwareAcceleration: 'no-preference' };
    this.stats.codec = codec;
    try {
      const support = await VideoDecoder.isConfigSupported(config);
      if (!support.supported) {
        this.stats.unsupported = true;
        this.publish(true);
        return;
      }
      const decoder = new VideoDecoder({
        output: (frame) => this.draw(frame),
        error: (e) => this.fail(e),
      });
      decoder.configure(config);
      this.decoder = decoder;
      this.codec = codec;
      this.waitingKey = true;
    } catch (e) {
      this.fail(e);
    } finally {
      this.configuring = false;
    }
    const pending = this.pending;
    this.pending = null;
    if (pending && this.decoder) this.push(pending);
  }

  private draw(frame: VideoFrame): void {
    const w = frame.displayWidth;
    const h = frame.displayHeight;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.ctx?.drawImage(frame, 0, 0, w, h);
    frame.close();
    this.stats.width = w;
    this.stats.height = h;
    this.stats.decoded++;
    const now = performance.now();
    this.frameTimes.push(now);
    while (this.frameTimes.length && now - this.frameTimes[0] > 1000) this.frameTimes.shift();
    this.publish();
  }

  private fail(e: unknown): void {
    console.warn('erro no decodificador de vídeo', e);
    this.stats.errors++;
    this.closeDecoder();
    this.codec = null;
    this.waitingKey = true;
    this.publish(true);
  }

  private closeDecoder(): void {
    if (this.decoder && this.decoder.state !== 'closed') {
      try {
        this.decoder.close();
      } catch {
        // já fechado pelo próprio erro
      }
    }
    this.decoder = null;
  }

  private publish(force = false): void {
    const now = performance.now();
    if (!force && now - this.lastStats < 500) return;
    this.lastStats = now;
    this.stats.fps = this.frameTimes.length;
    this.onStats({ ...this.stats });
  }
}
