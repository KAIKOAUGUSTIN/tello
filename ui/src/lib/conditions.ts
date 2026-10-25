// Rótulos e ícones das condições de voo (ver crates/tello-core/src/conditions.rs).

import type { IconName } from './icons';
import type { Condition } from './types';

export const CONDITIONS: Record<Condition, { label: string; icon: IconName }> = {
  low_light: { label: 'Pouca luz', icon: 'sun-dim' },
  wind: { label: 'Vento forte', icon: 'wind' },
  weak_signal: { label: 'Sinal Wi-Fi fraco', icon: 'wifi-low' },
  high_altitude: { label: 'Acima de 6 m', icon: 'mountain' },
};
