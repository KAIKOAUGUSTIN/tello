// Relógio reativo de baixa frequência (cronômetros da UI).

export const clock = $state({ now: performance.now() });

setInterval(() => (clock.now = performance.now()), 500);

export function elapsed(since: number): string {
  const s = Math.max(0, Math.floor((clock.now - since) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}
