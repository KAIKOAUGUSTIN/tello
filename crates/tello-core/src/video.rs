//! Remontagem do fluxo H.264 (Annex B) que chega fragmentado em pacotes UDP.
//!
//! Os bytes são acumulados, cortados em NAL units pelos start codes e
//! agrupados em access units (um frame cada), que é o que o decodificador
//! da UI consome.

#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    /// Access unit completa em Annex B (com start codes de 4 bytes).
    pub data: Vec<u8>,
    /// Contém um IDR: o decodificador pode começar daqui.
    pub keyframe: bool,
    /// Contém SPS/PPS.
    pub has_sps: bool,
    pub seq: u64,
}

const START: [u8; 4] = [0, 0, 0, 1];
const MAX_BUFFER: usize = 4 * 1024 * 1024;

pub const NAL_SLICE: u8 = 1;
pub const NAL_IDR: u8 = 5;
pub const NAL_SEI: u8 = 6;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
pub const NAL_AUD: u8 = 9;

#[derive(Default)]
pub struct NalAssembler {
    buf: Vec<u8>,
    au: Vec<u8>,
    au_has_vcl: bool,
    au_key: bool,
    au_sps: bool,
    /// Descarta frames até chegar um keyframe (início ou após perda).
    waiting_key: bool,
    seq: u64,
    dropped: u64,
}

impl NalAssembler {
    pub fn new() -> Self {
        NalAssembler { waiting_key: true, ..Default::default() }
    }

    /// Acrescenta bytes do fluxo e devolve os frames que ficaram completos.
    pub fn push(&mut self, data: &[u8]) -> Vec<VideoFrame> {
        let mut out = Vec::new();
        self.buf.extend_from_slice(data);
        if self.buf.len() > MAX_BUFFER {
            self.reset();
            return out;
        }
        self.drain_nals(false, &mut out);
        out
    }

    /// Indica que o frame atual terminou (ex.: último fragmento recebido).
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        let mut out = Vec::new();
        self.drain_nals(true, &mut out);
        self.finish_au(&mut out);
        out
    }

    /// Descarta dados parciais após perda de pacotes e espera o próximo keyframe.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.clear_au();
        self.waiting_key = true;
    }

    /// Frames descartados enquanto se esperava um keyframe.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    fn drain_nals(&mut self, at_end: bool, out: &mut Vec<VideoFrame>) {
        let Some(first) = find_start(&self.buf, 0) else {
            if at_end {
                self.buf.clear();
            }
            return;
        };
        let mut pos = first;
        loop {
            let payload = pos + start_len(&self.buf, pos);
            match find_start(&self.buf, payload) {
                Some(next) => {
                    let nal = self.buf[payload..next].to_vec();
                    self.add_nal(&nal, out);
                    pos = next;
                }
                None => {
                    if at_end {
                        let nal = self.buf[payload..].to_vec();
                        self.add_nal(&nal, out);
                        self.buf.clear();
                    } else {
                        self.buf.drain(..pos);
                    }
                    return;
                }
            }
        }
    }

    fn add_nal(&mut self, nal: &[u8], out: &mut Vec<VideoFrame>) {
        let nal = trim_trailing_zeros(nal);
        if nal.is_empty() {
            return;
        }
        let kind = nal[0] & 0x1f;
        let is_vcl = (NAL_SLICE..=NAL_IDR).contains(&kind);
        if self.au_has_vcl {
            let starts_new = match kind {
                NAL_SPS | NAL_PPS | NAL_AUD | NAL_SEI => true,
                // first_mb_in_slice == 0 (ue(v) começando com bit 1) abre um novo frame.
                _ if is_vcl => nal.len() > 1 && nal[1] & 0x80 != 0,
                _ => false,
            };
            if starts_new {
                self.finish_au(out);
            }
        }
        self.au.extend_from_slice(&START);
        self.au.extend_from_slice(nal);
        self.au_has_vcl |= is_vcl;
        self.au_key |= kind == NAL_IDR;
        self.au_sps |= kind == NAL_SPS;
    }

    fn finish_au(&mut self, out: &mut Vec<VideoFrame>) {
        if !self.au_has_vcl {
            // Só parâmetros (SPS/PPS) até agora: continuam no próximo frame.
            return;
        }
        if self.waiting_key && !self.au_key {
            self.dropped += 1;
            self.clear_au();
            return;
        }
        self.waiting_key = false;
        self.seq += 1;
        out.push(VideoFrame {
            data: std::mem::take(&mut self.au),
            keyframe: self.au_key,
            has_sps: self.au_sps,
            seq: self.seq,
        });
        self.clear_au();
    }

    fn clear_au(&mut self) {
        self.au.clear();
        self.au_has_vcl = false;
        self.au_key = false;
        self.au_sps = false;
    }
}

/// Posição do próximo start code (3 ou 4 bytes) a partir de `from`.
fn find_start(buf: &[u8], from: usize) -> Option<usize> {
    if buf.len() < 3 {
        return None;
    }
    let mut i = from;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 {
            if buf[i + 2] == 1 {
                return Some(if i > from && buf[i - 1] == 0 { i - 1 } else { i });
            }
            if buf[i + 2] == 0 && i + 3 < buf.len() && buf[i + 3] == 1 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn start_len(buf: &[u8], pos: usize) -> usize {
    if buf.len() >= pos + 4 && buf[pos..pos + 4] == START {
        4
    } else {
        3
    }
}

fn trim_trailing_zeros(nal: &[u8]) -> &[u8] {
    let mut end = nal.len();
    while end > 0 && nal[end - 1] == 0 {
        end -= 1;
    }
    &nal[..end]
}

/// Informações básicas de um SPS (Sequence Parameter Set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpsInfo {
    pub profile_idc: u8,
    pub constraints: u8,
    pub level_idc: u8,
    pub width: u32,
    pub height: u32,
}

impl SpsInfo {
    /// Codec string no formato do WebCodecs/MP4, ex.: `avc1.4D4028`.
    pub fn codec_string(&self) -> String {
        format!("avc1.{:02X}{:02X}{:02X}", self.profile_idc, self.constraints, self.level_idc)
    }

    pub fn profile_name(&self) -> &'static str {
        match self.profile_idc {
            66 => "Baseline",
            77 => "Main",
            88 => "Extended",
            100 => "High",
            _ => "outro",
        }
    }
}

/// Lê perfil, nível e resolução de uma NAL SPS (com o byte de cabeçalho).
pub fn parse_sps(nal: &[u8]) -> Option<SpsInfo> {
    if nal.len() < 4 || nal[0] & 0x1f != NAL_SPS {
        return None;
    }
    let rbsp = remove_emulation_prevention(&nal[1..]);
    let mut r = BitReader::new(&rbsp);
    let profile_idc = r.bits(8)? as u8;
    let constraints = r.bits(8)? as u8;
    let level_idc = r.bits(8)? as u8;
    r.ue()?; // seq_parameter_set_id
    let mut chroma_format_idc = 1;
    if matches!(profile_idc, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135) {
        chroma_format_idc = r.ue()?;
        if chroma_format_idc == 3 {
            r.bits(1)?; // separate_colour_plane_flag
        }
        r.ue()?; // bit_depth_luma_minus8
        r.ue()?; // bit_depth_chroma_minus8
        r.bits(1)?; // qpprime_y_zero_transform_bypass_flag
        if r.bits(1)? == 1 {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for i in 0..lists {
                if r.bits(1)? == 1 {
                    skip_scaling_list(&mut r, if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    r.ue()?; // log2_max_frame_num_minus4
    let poc_type = r.ue()?;
    if poc_type == 0 {
        r.ue()?;
    } else if poc_type == 1 {
        r.bits(1)?;
        r.se()?;
        r.se()?;
        for _ in 0..r.ue()? {
            r.se()?;
        }
    }
    r.ue()?; // max_num_ref_frames
    r.bits(1)?; // gaps_in_frame_num_value_allowed_flag
    let width_mbs = r.ue()? + 1;
    let height_map_units = r.ue()? + 1;
    let frame_mbs_only = r.bits(1)?;
    if frame_mbs_only == 0 {
        r.bits(1)?; // mb_adaptive_frame_field_flag
    }
    r.bits(1)?; // direct_8x8_inference_flag
    let mut width = width_mbs * 16;
    let mut height = (2 - frame_mbs_only) * height_map_units * 16;
    if r.bits(1)? == 1 {
        let (left, right, top, bottom) = (r.ue()?, r.ue()?, r.ue()?, r.ue()?);
        let (crop_x, crop_y) = match chroma_format_idc {
            0 => (1, 2 - frame_mbs_only),
            1 => (2, 2 * (2 - frame_mbs_only)),
            2 => (2, 2 - frame_mbs_only),
            _ => (1, 2 - frame_mbs_only),
        };
        width = width.checked_sub((left + right) * crop_x)?;
        height = height.checked_sub((top + bottom) * crop_y)?;
    }
    Some(SpsInfo { profile_idc, constraints, level_idc, width, height })
}

/// Primeira NAL SPS de um frame Annex B, se houver.
pub fn find_sps(annexb: &[u8]) -> Option<&[u8]> {
    nal_units(annexb).find(|n| n[0] & 0x1f == NAL_SPS)
}

/// Itera as NAL units (sem start code) de um buffer Annex B.
pub fn nal_units(annexb: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut pos = find_start(annexb, 0);
    std::iter::from_fn(move || {
        let s = pos?;
        let p = s + start_len(annexb, s);
        let next = find_start(annexb, p);
        pos = next;
        let nal = trim_trailing_zeros(&annexb[p..next.unwrap_or(annexb.len())]);
        Some(nal)
    })
    .filter(|n| !n.is_empty())
}

fn skip_scaling_list(r: &mut BitReader, size: usize) -> Option<()> {
    let (mut last, mut next) = (8i64, 8i64);
    for _ in 0..size {
        if next != 0 {
            next = (last + r.se()? + 256) % 256;
        }
        if next != 0 {
            last = next;
        }
    }
    Some(())
}

pub(crate) fn remove_emulation_prevention(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0 }
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.data.get(self.pos / 8)?;
            let bit = (byte >> (7 - self.pos % 8)) & 1;
            v = (v << 1) | bit as u32;
            self.pos += 1;
        }
        Some(v)
    }

    /// Exp-Golomb sem sinal.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bits(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.bits(zeros)?)
    }

    /// Exp-Golomb com sinal.
    fn se(&mut self) -> Option<i64> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) })
    }
}

/// Tipos de NAL presentes num frame Annex B (útil para logs e testes).
pub fn nal_types(annexb: &[u8]) -> Vec<u8> {
    nal_units(annexb).map(|n| n[0] & 0x1f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nal(kind: u8, first_mb_zero: bool, len: usize) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 1, 0x60 | kind];
        v.push(if first_mb_zero { 0x88 } else { 0x40 });
        v.extend((0..len).map(|i| (i % 200 + 1) as u8));
        v
    }

    fn stream() -> Vec<u8> {
        let mut s = Vec::new();
        s.extend(nal(NAL_SPS, true, 10));
        s.extend(nal(NAL_PPS, true, 4));
        s.extend(nal(NAL_IDR, true, 3000));
        s.extend(nal(NAL_SLICE, true, 800));
        s.extend(nal(NAL_SLICE, true, 700));
        s
    }

    #[test]
    fn splits_frames_across_fragments() {
        let s = stream();
        let mut asm = NalAssembler::new();
        let mut frames = Vec::new();
        for chunk in s.chunks(1460) {
            frames.extend(asm.push(chunk));
        }
        frames.extend(asm.flush());
        assert_eq!(frames.len(), 3);
        assert!(frames[0].keyframe && frames[0].has_sps);
        assert_eq!(nal_types(&frames[0].data), [NAL_SPS, NAL_PPS, NAL_IDR]);
        assert_eq!(nal_types(&frames[1].data), [NAL_SLICE]);
        assert!(!frames[2].keyframe);
        assert_eq!(frames.iter().map(|f| f.seq).collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn multi_slice_frame_stays_together() {
        let mut s = Vec::new();
        s.extend(nal(NAL_SPS, true, 10));
        s.extend(nal(NAL_PPS, true, 4));
        s.extend(nal(NAL_IDR, true, 500));
        s.extend(nal(NAL_IDR, false, 500)); // segunda fatia do mesmo frame
        s.extend(nal(NAL_SLICE, true, 100));
        let mut asm = NalAssembler::new();
        let mut frames = asm.push(&s);
        frames.extend(asm.flush());
        assert_eq!(frames.len(), 2);
        assert_eq!(nal_types(&frames[0].data), [NAL_SPS, NAL_PPS, NAL_IDR, NAL_IDR]);
    }

    #[test]
    fn drops_until_keyframe() {
        let mut s = nal(NAL_SLICE, true, 100);
        s.extend(nal(NAL_SLICE, true, 100));
        s.extend(stream());
        let mut asm = NalAssembler::new();
        let mut frames = asm.push(&s);
        frames.extend(asm.flush());
        assert_eq!(frames.len(), 3);
        assert!(frames[0].keyframe);
        assert_eq!(asm.dropped(), 2);
    }

    #[test]
    fn reset_waits_for_next_keyframe() {
        let mut asm = NalAssembler::new();
        let mut frames = asm.push(&stream());
        frames.extend(asm.flush());
        assert_eq!(frames.len(), 3);
        asm.push(&nal(NAL_SLICE, true, 50)[..20]);
        asm.reset();
        let mut more = asm.push(&nal(NAL_SLICE, true, 50));
        more.extend(asm.flush());
        assert!(more.is_empty());
    }

    #[test]
    fn three_byte_start_codes() {
        let mut s = vec![0, 0, 1, 0x67, 0x88, 1, 2];
        s.extend([0, 0, 1, 0x68, 0x88, 3]);
        s.extend([0, 0, 1, 0x65, 0x88, 4, 5, 6]);
        let mut asm = NalAssembler::new();
        let mut frames = asm.push(&s);
        frames.extend(asm.flush());
        assert_eq!(frames.len(), 1);
        assert_eq!(nal_types(&frames[0].data), [NAL_SPS, NAL_PPS, NAL_IDR]);
        assert!(frames[0].data.starts_with(&START));
    }
}
