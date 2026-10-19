export function num(v: number | null | undefined, digits = 0): string {
  return v === null || v === undefined || Number.isNaN(v) ? '—' : v.toFixed(digits);
}

export function meters(cm: number | null | undefined): string {
  return cm === null || cm === undefined ? '—' : (cm / 100).toFixed(1);
}

/** Velocidade horizontal em m/s a partir de dm/s. */
export function groundSpeed(x: number | null, y: number | null): string {
  if (x === null || y === null) return '—';
  return (Math.hypot(x, y) / 10).toFixed(1);
}

export function duration(s: number | null | undefined): string {
  if (s === null || s === undefined) return '—';
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
}

/** Carimbo de data/hora local para nomes de arquivo: 20261019-153012. */
export function stamp(d = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

const MODEL_NAMES: Record<string, string> = {
  tello: 'Tello',
  tello_edu: 'Tello EDU',
  robo_master_tt: 'RoboMaster TT',
  unknown: 'Tello',
};

export function modelName(model: string): string {
  return MODEL_NAMES[model] ?? model;
}
