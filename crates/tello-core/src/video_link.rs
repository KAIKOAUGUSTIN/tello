//! Saúde do link de vídeo no modo App: perda de pacotes medida pelo cabeçalho
//! de 2 bytes e ajuste automático da taxa do codificador do drone.

/// Taxa de cada nível do drone, em Mbps (o nível 0 deixa o drone escolher).
pub const LEVEL_MBPS: [f32; 6] = [0.0, 1.0, 1.5, 2.0, 3.0, 4.0];
pub const MAX_LEVEL: u8 = 5;

/// Perda acima da qual a janela conta como ruim.
const BAD_LOSS: f32 = 0.05;
/// Perda abaixo da qual a janela conta como limpa.
const CLEAN_LOSS: f32 = 0.01;
/// Janelas ruins seguidas para baixar um nível.
const BAD_WINDOWS: u32 = 2;
/// Janelas limpas seguidas para subir um nível.
const CLEAN_WINDOWS: u32 = 15;
/// Janelas ignoradas depois de cada troca, enquanto o codificador se ajusta.
const HOLD_WINDOWS: u32 = 3;
/// Com menos pacotes que isso na janela (vídeo parado), nada é decidido.
const MIN_PACKETS: u32 = 20;

/// Conta os pacotes perdidos pelo cabeçalho `[nº do frame][fragmento | 0x80 no último]`.
#[derive(Clone, Debug, Default)]
pub struct FrameGap {
    prev: Option<(u8, u8, bool)>,
    /// Média de fragmentos por frame, para estimar frames perdidos inteiros.
    avg_fragments: f32,
}

impl FrameGap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registra um pacote e devolve quantos se perderam antes dele.
    pub fn push(&mut self, header: [u8; 2]) -> u32 {
        let (frame, frag, last) = (header[0], header[1] & 0x7f, header[1] & 0x80 != 0);
        let lost = match self.prev {
            None => 0,
            Some((pf, pfrag, _)) if frame == pf => frag.saturating_sub(pfrag).saturating_sub(1) as u32,
            Some((pf, _, plast)) => {
                let skipped = frame.wrapping_sub(pf).wrapping_sub(1);
                if skipped >= 128 {
                    // Pacote atrasado de um frame anterior: só ignora.
                    return 0;
                }
                let per_frame = self.avg_fragments.max(1.0).round() as u32;
                skipped as u32 * per_frame + u32::from(!plast) + frag as u32
            }
        };
        if last {
            let n = frag as f32 + 1.0;
            self.avg_fragments = if self.avg_fragments == 0.0 { n } else { self.avg_fragments * 0.875 + n * 0.125 };
        }
        self.prev = Some((frame, frag, last));
        lost
    }
}

/// Escolhe o nível de taxa pelo histórico de perda, janela a janela (1 s).
#[derive(Clone, Debug)]
pub struct BitrateGovernor {
    level: u8,
    max: u8,
    bad: u32,
    clean: u32,
    hold: u32,
}

impl BitrateGovernor {
    /// Começa no teto: com Wi-Fi bom, a melhor imagem.
    pub fn new(max: u8) -> Self {
        let max = max.clamp(1, MAX_LEVEL);
        BitrateGovernor { level: max, max, bad: 0, clean: 0, hold: 0 }
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    /// Fecha uma janela e devolve o novo nível quando ele muda.
    pub fn tick(&mut self, received: u32, lost: u32) -> Option<u8> {
        if self.hold > 0 {
            self.hold -= 1;
            return None;
        }
        let total = received + lost;
        if total < MIN_PACKETS {
            self.bad = 0;
            return None;
        }
        let loss = lost as f32 / total as f32;
        if loss > BAD_LOSS {
            self.clean = 0;
            self.bad += 1;
            if self.bad >= BAD_WINDOWS && self.level > 1 {
                return Some(self.change(self.level - 1));
            }
        } else {
            self.bad = 0;
            self.clean = if loss < CLEAN_LOSS { self.clean + 1 } else { 0 };
            if self.clean >= CLEAN_WINDOWS && self.level < self.max {
                return Some(self.change(self.level + 1));
            }
        }
        None
    }

    fn change(&mut self, level: u8) -> u8 {
        self.level = level;
        self.bad = 0;
        self.clean = 0;
        self.hold = HOLD_WINDOWS;
        level
    }
}

/// Perda em % de uma janela.
pub fn loss_pct(received: u32, lost: u32) -> f32 {
    let total = received + lost;
    if total == 0 {
        0.0
    } else {
        lost as f32 * 100.0 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(gap: &mut FrameGap, n: u8, fragments: u8) -> u32 {
        (0..fragments).map(|k| gap.push([n, k | if k + 1 == fragments { 0x80 } else { 0 }])).sum()
    }

    #[test]
    fn no_loss_in_sequence_and_across_wraparound() {
        let mut gap = FrameGap::new();
        let mut lost = 0;
        for i in 0..300u32 {
            lost += frame(&mut gap, i as u8, 4);
        }
        assert_eq!(lost, 0);
    }

    #[test]
    fn counts_missing_fragments() {
        let mut gap = FrameGap::new();
        frame(&mut gap, 0, 4);
        // Frame 1 sem os fragmentos 1 e 2.
        assert_eq!(gap.push([1, 0]), 0);
        assert_eq!(gap.push([1, 3 | 0x80]), 2);
        // Frame 2 sem o último fragmento, frame 3 sem o primeiro.
        gap.push([2, 0]);
        gap.push([2, 1]);
        assert_eq!(gap.push([3, 1]), 2, "fim do 2 e começo do 3");
    }

    #[test]
    fn estimates_whole_frames_lost() {
        let mut gap = FrameGap::new();
        for i in 0..10 {
            frame(&mut gap, i, 3);
        }
        // Frames 10 e 11 inteiros perdidos.
        assert_eq!(gap.push([12, 0]), 6);
    }

    #[test]
    fn ignores_late_packets() {
        let mut gap = FrameGap::new();
        frame(&mut gap, 5, 2);
        frame(&mut gap, 6, 2);
        assert_eq!(gap.push([5, 1 | 0x80]), 0);
        assert_eq!(frame(&mut gap, 7, 2), 0);
    }

    #[test]
    fn governor_steps_down_after_two_bad_windows() {
        let mut g = BitrateGovernor::new(4);
        assert_eq!(g.tick(90, 10), None, "uma janela ruim não basta");
        assert_eq!(g.tick(90, 10), Some(3));
        // Carência: nada muda nas próximas 3 janelas.
        for _ in 0..3 {
            assert_eq!(g.tick(50, 50), None);
        }
        assert_eq!(g.tick(80, 20), None);
        assert_eq!(g.tick(80, 20), Some(2));
    }

    #[test]
    fn governor_climbs_back_after_clean_windows() {
        let mut g = BitrateGovernor::new(3);
        g.tick(80, 20);
        assert_eq!(g.tick(80, 20), Some(2));
        let mut changes = Vec::new();
        for _ in 0..(3 + 15) {
            changes.extend(g.tick(100, 0));
        }
        assert_eq!(changes, [3]);
        for _ in 0..40 {
            assert_eq!(g.tick(100, 0), None, "não passa do teto");
        }
    }

    #[test]
    fn governor_waits_when_video_is_idle_and_respects_floor() {
        let mut g = BitrateGovernor::new(1);
        for _ in 0..5 {
            assert_eq!(g.tick(5, 5), None, "poucos pacotes");
            assert_eq!(g.tick(10, 90), None, "já está no mínimo");
        }
        assert_eq!(g.level(), 1);
        assert_eq!(BitrateGovernor::new(0).level(), 1);
        assert_eq!(BitrateGovernor::new(9).level(), MAX_LEVEL);
    }
}
