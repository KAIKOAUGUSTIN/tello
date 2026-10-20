//! Transferência das fotos em alta resolução.
//!
//! 1. O drone anuncia o arquivo (FILE_SIZE 0x62): tipo, tamanho LE32, número LE16.
//! 2. Envia fragmentos (FILE_DATA 0x63): número LE16, chunk LE32, fragmento LE32,
//!    tamanho LE16 e os dados. Cada chunk tem até 8 fragmentos de 1024 bytes.
//! 3. O cliente confirma cada chunk completo e, no fim, confirma o arquivo.

use std::collections::BTreeMap;

use super::packet::{le_u32, Packet};

pub const FRAGMENT: usize = 1024;
pub const FRAGMENTS_PER_CHUNK: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Announce {
    pub kind: u8,
    pub size: u32,
    pub file_id: u16,
}

pub fn parse_announce(p: &[u8]) -> Option<Announce> {
    Some(Announce { kind: *p.first()?, size: le_u32(p, 1)?, file_id: u16::from_le_bytes([*p.get(5)?, *p.get(6)?]) })
}

pub fn encode_announce(a: Announce) -> Vec<u8> {
    let mut p = vec![a.kind];
    p.extend_from_slice(&a.size.to_le_bytes());
    p.extend_from_slice(&a.file_id.to_le_bytes());
    p
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub file_id: u16,
    pub chunk: u32,
    pub index: u32,
    pub data: Vec<u8>,
}

pub fn parse_fragment(p: &[u8]) -> Option<Fragment> {
    let file_id = u16::from_le_bytes([*p.first()?, *p.get(1)?]);
    let chunk = le_u32(p, 2)?;
    let index = le_u32(p, 6)?;
    let len = u16::from_le_bytes([*p.get(10)?, *p.get(11)?]) as usize;
    let data = p.get(12..12 + len)?.to_vec();
    Some(Fragment { file_id, chunk, index, data })
}

pub fn encode_fragment(f: &Fragment) -> Vec<u8> {
    let mut p = Vec::with_capacity(12 + f.data.len());
    p.extend_from_slice(&f.file_id.to_le_bytes());
    p.extend_from_slice(&f.chunk.to_le_bytes());
    p.extend_from_slice(&f.index.to_le_bytes());
    p.extend_from_slice(&(f.data.len() as u16).to_le_bytes());
    p.extend_from_slice(&f.data);
    p
}

/// Divide um arquivo em fragmentos, como o drone faz (simulador e testes).
pub fn split(file_id: u16, data: &[u8]) -> Vec<Fragment> {
    data.chunks(FRAGMENT)
        .enumerate()
        .map(|(i, d)| Fragment { file_id, chunk: (i / FRAGMENTS_PER_CHUNK) as u32, index: i as u32, data: d.to_vec() })
        .collect()
}

/// Confirmação de chunk recebido: FILE_DATA tipo 0x50, payload [0, id LE16, chunk LE32].
pub fn chunk_ack(file_id: u16, chunk: u32) -> Packet {
    Packet::new(super::packet::kind::DATA1, super::packet::id::FILE_DATA, ack_payload(0, file_id, chunk))
}

/// Confirmações finais: FILE_SIZE [1, id, chunk] e FILE_COMPLETE [id, tamanho].
pub fn completion_acks(file_id: u16, last_chunk: u32, size: u32) -> [Packet; 2] {
    let mut done = file_id.to_le_bytes().to_vec();
    done.extend_from_slice(&size.to_le_bytes());
    [
        Packet::new(super::packet::kind::DATA1, super::packet::id::FILE_SIZE, ack_payload(1, file_id, last_chunk)),
        Packet::new(super::packet::kind::GET, super::packet::id::FILE_COMPLETE, done),
    ]
}

fn ack_payload(flag: u8, file_id: u16, chunk: u32) -> Vec<u8> {
    let mut p = vec![flag];
    p.extend_from_slice(&file_id.to_le_bytes());
    p.extend_from_slice(&chunk.to_le_bytes());
    p
}

/// Monta o arquivo a partir dos fragmentos, aceitando repetições e fora de ordem.
pub struct Download {
    pub announce: Announce,
    fragments: BTreeMap<(u32, u32), Vec<u8>>,
    received: usize,
}

pub enum Progress {
    /// Fragmento repetido ou de outro arquivo.
    Ignored,
    Stored,
    /// Este fragmento completou o chunk indicado (deve ser confirmado).
    ChunkDone(u32),
}

impl Download {
    pub fn new(announce: Announce) -> Self {
        Download { announce, fragments: BTreeMap::new(), received: 0 }
    }

    fn total_fragments(&self) -> usize {
        (self.announce.size as usize).div_ceil(FRAGMENT)
    }

    fn expected_in_chunk(&self, chunk: u32) -> usize {
        let before = chunk as usize * FRAGMENTS_PER_CHUNK;
        self.total_fragments().saturating_sub(before).min(FRAGMENTS_PER_CHUNK)
    }

    pub fn add(&mut self, f: Fragment) -> Progress {
        if f.file_id != self.announce.file_id || self.fragments.contains_key(&(f.chunk, f.index)) {
            return Progress::Ignored;
        }
        self.received += f.data.len();
        let chunk = f.chunk;
        self.fragments.insert((f.chunk, f.index), f.data);
        let in_chunk = self.fragments.range((chunk, 0)..=(chunk, u32::MAX)).count();
        if in_chunk == self.expected_in_chunk(chunk) {
            Progress::ChunkDone(chunk)
        } else {
            Progress::Stored
        }
    }

    pub fn is_complete(&self) -> bool {
        self.received >= self.announce.size as usize
    }

    pub fn progress(&self) -> f32 {
        self.received as f32 / self.announce.size.max(1) as f32
    }

    pub fn last_chunk(&self) -> u32 {
        self.fragments.keys().next_back().map(|k| k.0).unwrap_or(0)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut out: Vec<u8> = self.fragments.into_values().flatten().collect();
        out.truncate(self.announce.size as usize);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembles_out_of_order_with_duplicates() {
        let file: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let frags = split(3, &file);
        assert_eq!(frags.len(), 20);
        let mut dl = Download::new(Announce { kind: 1, size: file.len() as u32, file_id: 3 });
        let mut acked = Vec::new();
        for f in frags.iter().rev().chain(frags.iter().take(2)) {
            let parsed = parse_fragment(&encode_fragment(f)).unwrap();
            if let Progress::ChunkDone(c) = dl.add(parsed) {
                acked.push(c);
            }
        }
        acked.sort();
        assert_eq!(acked, [0, 1, 2], "três chunks: 8 + 8 + 4 fragmentos");
        assert!(dl.is_complete());
        assert_eq!(dl.into_bytes(), file);
    }

    #[test]
    fn announce_roundtrip() {
        let a = Announce { kind: 1, size: 1_234_567, file_id: 9 };
        assert_eq!(parse_announce(&encode_announce(a)), Some(a));
    }
}
