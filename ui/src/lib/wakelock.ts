// Mantém a tela acesa durante o voo (Screen Wake Lock API). No celular o
// plugin nativo já faz isso; aqui é um reforço para desktop e navegador.

export function keepScreenOn(): () => void {
  if (!('wakeLock' in navigator)) return () => {};
  let lock: WakeLockSentinel | null = null;
  let stopped = false;

  const acquire = async () => {
    if (stopped || lock || document.visibilityState !== 'visible') return;
    try {
      const l = await navigator.wakeLock.request('screen');
      if (stopped) return void l.release();
      lock = l;
      l.addEventListener('release', () => {
        if (lock === l) lock = null;
      });
    } catch {
      // sem permissão ou sem suporte: segue sem
    }
  };

  // O navegador solta a trava quando a janela some; pega de novo ao voltar.
  const onVisibility = () => void acquire();
  document.addEventListener('visibilitychange', onVisibility);
  void acquire();

  return () => {
    stopped = true;
    document.removeEventListener('visibilitychange', onVisibility);
    void lock?.release();
    lock = null;
  };
}
