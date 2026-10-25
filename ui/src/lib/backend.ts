import type {
  Action,
  Capabilities,
  Config,
  DroneEvent,
  LogEntry,
  NetworkCheck,
  PlatformInfo,
  Protocol,
  RecordingResult,
  Safety,
  Sticks,
  VideoPacket,
  VideoPolicy,
} from './types';

export type FolderKind = 'photos' | 'videos' | 'logs';
type Unsubscribe = () => void;

/** Tudo que a UI precisa do lado nativo. Há duas implementações: Tauri e simulada. */
export interface Backend {
  readonly kind: 'tauri' | 'mock';
  platform(): Promise<PlatformInfo>;
  networkCheck(ip: string): Promise<NetworkCheck>;
  connect(protocol: Protocol, config: Config): Promise<Capabilities>;
  disconnect(): Promise<void>;
  action(action: Action): Promise<string>;
  setSticks(sticks: Sticks): void;
  setSafety(safety: Safety): void;
  setVideoPolicy(policy: VideoPolicy): void;
  onEvent(cb: (e: DroneEvent) => void): Unsubscribe;
  onVideo(cb: (p: VideoPacket) => void): Unsubscribe;
  onLog(cb: (entries: LogEntry[]) => void): Unsubscribe;
  onCloseRequest(cb: () => void): Unsubscribe;
  logSnapshot(): Promise<LogEntry[]>;
  exportLog(stamp: string): Promise<string>;
  saveSnapshot(png: Uint8Array, stamp: string): Promise<string>;
  /** Começa a gravar o vídeo em MP4; devolve o caminho do arquivo. */
  startRecording(stamp: string): Promise<string>;
  stopRecording(): Promise<RecordingResult>;
  openFolder(kind: FolderKind): Promise<void>;
  /** Abre o relatório de um voo (HTML da pasta de logs) no navegador. */
  openReport(path: string): Promise<void>;
  /** Anota um evento da interface na caixa-preta. */
  logNote(text: string): Promise<void>;
  landAndQuit(): Promise<void>;
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** Decodifica o pacote de vídeo binário: [flags u8][seq u32 LE][Annex B]. */
export function decodeVideoPacket(buf: ArrayBuffer): VideoPacket {
  const bytes = new Uint8Array(buf);
  const view = new DataView(buf);
  return {
    key: (bytes[0] & 1) !== 0,
    hasSps: (bytes[0] & 2) !== 0,
    seq: view.getUint32(1, true),
    data: bytes.subarray(5),
  };
}

/** Pequeno emissor de eventos usado pelas implementações. */
export class Emitter<T> {
  private listeners = new Set<(v: T) => void>();

  on(cb: (v: T) => void): Unsubscribe {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  emit(v: T): void {
    for (const cb of this.listeners) cb(v);
  }
}

let instance: Promise<Backend> | null = null;

export function getBackend(): Promise<Backend> {
  instance ??= isTauri()
    ? import('./backend-tauri').then((m) => m.createTauriBackend())
    : import('./backend-mock').then((m) => new m.MockBackend());
  return instance;
}
