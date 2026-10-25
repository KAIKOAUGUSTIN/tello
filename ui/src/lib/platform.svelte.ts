// Plataforma em que o app roda e o layout da tela.

import type { Backend } from './backend';

/**
 * Telas baixas ou estreitas (celular) usam o layout compacto. A janela do
 * desktop nunca chega a isso: o tamanho mínimo dela é 960×600.
 */
const COMPACT_QUERY = '(max-height: 560px), (max-width: 760px)';

const compactQuery = typeof matchMedia === 'function' ? matchMedia(COMPACT_QUERY) : null;

export const platform = $state({
  os: 'web',
  mobile: false,
  compact: compactQuery?.matches ?? false,
});

/** Lê a plataforma do backend e acompanha as mudanças de tamanho da tela. */
export function startPlatform(backend: Backend): () => void {
  void backend.platform().then((info) => {
    platform.os = info.os;
    platform.mobile = info.mobile;
  });
  const onChange = (e: MediaQueryListEvent) => (platform.compact = e.matches);
  compactQuery?.addEventListener('change', onChange);
  return () => compactQuery?.removeEventListener('change', onChange);
}

/** Onde o usuário encontra as fotos e os vídeos no aparelho. */
export function mediaHint(): string {
  if (platform.os === 'ios') return 'Fotos e vídeos ficam no app Arquivos, em "Tello Pilot".';
  if (platform.os === 'android') return 'Fotos e vídeos ficam na galeria, na pasta "Tello Pilot".';
  return '';
}

/** Onde o usuário encontra os logs e os relatórios de voo no aparelho. */
export function logsHint(): string {
  if (platform.os === 'ios') return 'no app Arquivos, em "Tello Pilot" → "Logs"';
  if (platform.os === 'android') return 'na pasta "Documents/Tello Pilot" do aparelho';
  return '';
}

/** "celular" ou "computador", para os textos de ajuda. */
export function deviceWord(): string {
  return platform.mobile ? 'celular' : 'computador';
}
