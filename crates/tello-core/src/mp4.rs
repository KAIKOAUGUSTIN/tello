//! Gravação em MP4 sem FFmpeg: um muxer mínimo para uma faixa H.264, sem áudio.
//!
//! O arquivo é escrito como `ftyp` + `mdat` (amostras em formato AVCC) e o
//! `moov` com as tabelas vai no final, ao chamar [`Mp4Writer::finish`].

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::video::{nal_units, parse_sps, NAL_AUD, NAL_PPS, NAL_SPS};

const TIMESCALE: u32 = 90_000;
const MOVIE_TIMESCALE: u32 = 1000;
/// Duração usada quando não há referência (30 fps).
const DEFAULT_DELTA: u32 = TIMESCALE / 30;
/// Espaço mínimo entre frames: frames que chegam em rajada são espalhados.
const MIN_DELTA: u64 = TIMESCALE as u64 / 60;

struct Sample {
    size: u32,
    delta: u32,
    key: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordingInfo {
    pub path: PathBuf,
    pub frames: usize,
    pub duration: Duration,
}

pub struct Mp4Writer {
    out: BufWriter<File>,
    path: PathBuf,
    mdat_start: u64,
    first_sample: u64,
    written: u64,
    samples: Vec<Sample>,
    /// Instante (ticks) do primeiro frame e do último frame gravado.
    first_ticks: Option<u64>,
    last_pts: u64,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
}

impl Mp4Writer {
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut out = BufWriter::new(File::create(&path)?);
        let ftyp = boxed(b"ftyp", &cat(&[b"isom", &0x200u32.to_be_bytes(), b"isom", b"iso2", b"avc1", b"mp41"]));
        out.write_all(&ftyp)?;
        let mdat_start = ftyp.len() as u64;
        // mdat com tamanho de 64 bits, preenchido no final.
        out.write_all(&1u32.to_be_bytes())?;
        out.write_all(b"mdat")?;
        out.write_all(&0u64.to_be_bytes())?;
        Ok(Mp4Writer {
            out,
            path,
            mdat_start,
            first_sample: mdat_start + 16,
            written: 0,
            samples: Vec::new(),
            first_ticks: None,
            last_pts: 0,
            sps: None,
            pps: None,
        })
    }

    pub fn frames(&self) -> usize {
        self.samples.len()
    }

    /// Acrescenta um frame Annex B. `ts` é o instante do frame desde o início da
    /// gravação. Frames anteriores ao primeiro keyframe com SPS/PPS são ignorados.
    pub fn write_frame(&mut self, annexb: &[u8], keyframe: bool, ts: Duration) -> io::Result<()> {
        for nal in nal_units(annexb) {
            match nal[0] & 0x1f {
                NAL_SPS if self.sps.is_none() => self.sps = Some(nal.to_vec()),
                NAL_PPS if self.pps.is_none() => self.pps = Some(nal.to_vec()),
                _ => {}
            }
        }
        if self.samples.is_empty() && !(keyframe && self.sps.is_some() && self.pps.is_some()) {
            return Ok(());
        }
        let mut size = 0u32;
        for nal in nal_units(annexb) {
            // SPS/PPS ficam no avcC; AUD não é necessário dentro do MP4.
            if matches!(nal[0] & 0x1f, NAL_SPS | NAL_PPS | NAL_AUD) {
                continue;
            }
            self.out.write_all(&(nal.len() as u32).to_be_bytes())?;
            self.out.write_all(nal)?;
            size += 4 + nal.len() as u32;
        }
        if size == 0 {
            return Ok(());
        }
        let ticks = ts.as_micros() as u64 * TIMESCALE as u64 / 1_000_000;
        let first = *self.first_ticks.get_or_insert(ticks);
        if let Some(last) = self.samples.last_mut() {
            // Segue o relógio real, mas nunca encosta no frame anterior.
            let pts = (ticks.saturating_sub(first)).max(self.last_pts + MIN_DELTA);
            last.delta = (pts - self.last_pts) as u32;
            self.last_pts = pts;
        }
        self.written += size as u64;
        self.samples.push(Sample { size, delta: DEFAULT_DELTA, key: keyframe });
        Ok(())
    }

    /// Fecha o arquivo escrevendo as tabelas. Sem frames, o arquivo é apagado.
    pub fn finish(mut self) -> io::Result<RecordingInfo> {
        if self.samples.is_empty() {
            drop(self.out);
            let _ = std::fs::remove_file(&self.path);
            return Err(io::Error::other("nenhum frame gravado"));
        }
        // O último frame dura o mesmo que a média dos anteriores.
        if self.samples.len() > 1 {
            let n = self.samples.len() - 1;
            let avg = self.samples[..n].iter().map(|s| s.delta as u64).sum::<u64>() / n as u64;
            self.samples[n].delta = avg as u32;
        }
        let moov = self.moov();
        self.out.write_all(&moov)?;
        self.out.seek(SeekFrom::Start(self.mdat_start + 8))?;
        self.out.write_all(&(16 + self.written).to_be_bytes())?;
        self.out.flush()?;
        let ticks: u64 = self.samples.iter().map(|s| s.delta as u64).sum();
        Ok(RecordingInfo {
            path: self.path.clone(),
            frames: self.samples.len(),
            duration: Duration::from_micros(ticks * 1_000_000 / TIMESCALE as u64),
        })
    }

    fn moov(&self) -> Vec<u8> {
        let sps = self.sps.as_deref().unwrap_or_default();
        let pps = self.pps.as_deref().unwrap_or_default();
        let info = parse_sps(sps);
        let (width, height) = info.map(|i| (i.width, i.height)).unwrap_or((960, 720));
        let ticks: u64 = self.samples.iter().map(|s| s.delta as u64).sum();
        let movie_duration = (ticks * MOVIE_TIMESCALE as u64 / TIMESCALE as u64) as u32;

        let mvhd = full(
            b"mvhd",
            0,
            0,
            &cat(&[
                &[0u8; 8], // criação e modificação
                &MOVIE_TIMESCALE.to_be_bytes(),
                &movie_duration.to_be_bytes(),
                &0x0001_0000u32.to_be_bytes(), // velocidade 1.0
                &0x0100u16.to_be_bytes(),      // volume 1.0
                &[0u8; 10],
                &MATRIX,
                &[0u8; 24],
                &2u32.to_be_bytes(), // próximo track id
            ]),
        );
        let tkhd = full(
            b"tkhd",
            0,
            3,
            &cat(&[
                &[0u8; 8],
                &1u32.to_be_bytes(),
                &[0u8; 4],
                &movie_duration.to_be_bytes(),
                &[0u8; 8],
                &[0u8; 4], // layer, grupo
                &[0u8; 4], // volume, reservado
                &MATRIX,
                &(width << 16).to_be_bytes(),
                &(height << 16).to_be_bytes(),
            ]),
        );
        let mdhd = full(
            b"mdhd",
            0,
            0,
            &cat(&[
                &[0u8; 8],
                &TIMESCALE.to_be_bytes(),
                &(ticks as u32).to_be_bytes(),
                &0x55c4u16.to_be_bytes(), // idioma "und"
                &[0u8; 2],
            ]),
        );
        let hdlr = full(b"hdlr", 0, 0, &cat(&[&[0u8; 4], b"vide", &[0u8; 12], b"Tello Pilot\0"]));
        let vmhd = full(b"vmhd", 0, 1, &[0u8; 8]);
        let dref = full(b"dref", 0, 0, &cat(&[&1u32.to_be_bytes(), &full(b"url ", 0, 1, &[])]));
        let dinf = boxed(b"dinf", &dref);

        let mut avcc = vec![1, sps.get(1).copied().unwrap_or(77), sps.get(2).copied().unwrap_or(0)];
        avcc.extend_from_slice(&[sps.get(3).copied().unwrap_or(40), 0xff, 0xe1]);
        avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(sps);
        avcc.push(1);
        avcc.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(pps);
        if matches!(avcc[1], 100 | 110 | 122 | 144) {
            avcc.extend_from_slice(&[0xfd, 0xf8, 0xf8, 0]); // 4:2:0, 8 bits
        }
        let mut compressor = [0u8; 32];
        compressor[..12].copy_from_slice(b"\x0bTello Pilot");
        let avc1 = boxed(
            b"avc1",
            &cat(&[
                &[0u8; 6],
                &1u16.to_be_bytes(), // data reference index
                &[0u8; 16],
                &(width as u16).to_be_bytes(),
                &(height as u16).to_be_bytes(),
                &0x0048_0000u32.to_be_bytes(),
                &0x0048_0000u32.to_be_bytes(),
                &[0u8; 4],
                &1u16.to_be_bytes(), // frames por amostra
                &compressor,
                &0x0018u16.to_be_bytes(),
                &0xffffu16.to_be_bytes(),
                &boxed(b"avcC", &avcc),
            ]),
        );
        let stsd = full(b"stsd", 0, 0, &cat(&[&1u32.to_be_bytes(), &avc1]));

        let mut runs: Vec<(u32, u32)> = Vec::new();
        for s in &self.samples {
            match runs.last_mut() {
                Some((count, delta)) if *delta == s.delta => *count += 1,
                _ => runs.push((1, s.delta)),
            }
        }
        let stts = full(b"stts", 0, 0, &table(runs.len(), runs.iter().flat_map(|(c, d)| [*c, *d])));
        let keys: Vec<u32> =
            self.samples.iter().enumerate().filter(|(_, s)| s.key).map(|(i, _)| i as u32 + 1).collect();
        let stss = full(b"stss", 0, 0, &table(keys.len(), keys.iter().copied()));
        let stsc = full(b"stsc", 0, 0, &table(1, [1, self.samples.len() as u32, 1]));
        let stsz = full(
            b"stsz",
            0,
            0,
            &cat(&[&0u32.to_be_bytes(), &table(self.samples.len(), self.samples.iter().map(|s| s.size))]),
        );
        let co64 = full(b"co64", 0, 0, &cat(&[&1u32.to_be_bytes(), &self.first_sample.to_be_bytes()]));
        let stbl = boxed(b"stbl", &cat(&[&stsd, &stts, &stss, &stsc, &stsz, &co64]));
        let minf = boxed(b"minf", &cat(&[&vmhd, &dinf, &stbl]));
        let mdia = boxed(b"mdia", &cat(&[&mdhd, &hdlr, &minf]));
        let trak = boxed(b"trak", &cat(&[&tkhd, &mdia]));
        boxed(b"moov", &cat(&[&mvhd, &trak]))
    }
}

const MATRIX: [u8; 36] = {
    let mut m = [0u8; 36];
    m[1] = 1; // 0x00010000
    m[17] = 1; // 0x00010000
    m[32] = 0x40; // 0x40000000
    m
};

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn boxed(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + content.len());
    b.extend_from_slice(&(8 + content.len() as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(content);
    b
}

fn full(kind: &[u8; 4], version: u8, flags: u32, content: &[u8]) -> Vec<u8> {
    let mut c = vec![version];
    c.extend_from_slice(&flags.to_be_bytes()[1..]);
    c.extend_from_slice(content);
    boxed(kind, &c)
}

/// Tabela com contagem de entradas seguida dos valores u32.
fn table(entries: usize, values: impl IntoIterator<Item = u32>) -> Vec<u8> {
    let mut t = (entries as u32).to_be_bytes().to_vec();
    for v in values {
        t.extend_from_slice(&v.to_be_bytes());
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_is_identity() {
        assert_eq!(&MATRIX[..4], &0x0001_0000u32.to_be_bytes());
        assert_eq!(&MATRIX[16..20], &0x0001_0000u32.to_be_bytes());
        assert_eq!(&MATRIX[32..], &0x4000_0000u32.to_be_bytes());
    }

    #[test]
    fn empty_recording_is_removed() {
        let path = std::env::temp_dir().join(format!("tello-empty-{}.mp4", std::process::id()));
        let w = Mp4Writer::create(&path).unwrap();
        assert!(w.finish().is_err());
        assert!(!path.exists());
    }
}
