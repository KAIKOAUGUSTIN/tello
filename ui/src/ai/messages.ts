// Mensagens entre a interface e o worker de visão.

export type Delegate = 'GPU' | 'CPU';

export type ToWorker = { type: 'init' } | { type: 'frame'; bitmap: ImageBitmap; ts: number };

export type FromWorker =
  | { type: 'ready'; delegate: Delegate }
  | { type: 'error'; message: string }
  /** Uma pose por pessoa encontrada (33 × [x, y, visibilidade]). */
  | { type: 'result'; poses: Float32Array[]; ms: number };
