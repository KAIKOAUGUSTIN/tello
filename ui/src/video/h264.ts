// Utilidades mínimas de H.264 Annex B para alimentar o WebCodecs.

export const NAL = { SLICE: 1, IDR: 5, SEI: 6, SPS: 7, PPS: 8, AUD: 9 } as const;

/** Posições dos start codes (00 00 01 / 00 00 00 01): [início, tamanho]. */
function startCodes(buf: Uint8Array): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  for (let i = 0; i + 2 < buf.length; i++) {
    if (buf[i] === 0 && buf[i + 1] === 0) {
      if (buf[i + 2] === 1) {
        const four = i > 0 && buf[i - 1] === 0;
        out.push(four ? [i - 1, 4] : [i, 3]);
        i += 2;
      }
    }
  }
  return out;
}

/** NAL units (sem start code) de um buffer Annex B. */
export function splitNals(buf: Uint8Array): Uint8Array[] {
  const starts = startCodes(buf);
  const nals: Uint8Array[] = [];
  starts.forEach(([pos, len], i) => {
    const end = i + 1 < starts.length ? starts[i + 1][0] : buf.length;
    const nal = buf.subarray(pos + len, end);
    if (nal.length > 0) nals.push(nal);
  });
  return nals;
}

export function nalType(nal: Uint8Array): number {
  return nal[0] & 0x1f;
}

export function findSps(au: Uint8Array): Uint8Array | null {
  return splitNals(au).find((n) => nalType(n) === NAL.SPS) ?? null;
}

/** Codec string `avc1.PPCCLL` a partir dos 3 bytes após o cabeçalho do SPS. */
export function codecFromSps(sps: Uint8Array): string {
  const hex = (b: number) => b.toString(16).padStart(2, '0').toUpperCase();
  return `avc1.${hex(sps[1])}${hex(sps[2])}${hex(sps[3])}`;
}

export interface AccessUnit {
  data: Uint8Array;
  key: boolean;
  hasSps: boolean;
}

/** Agrupa um fluxo Annex B completo em frames (mesma regra do tello-core). */
export function accessUnits(stream: Uint8Array): AccessUnit[] {
  const out: AccessUnit[] = [];
  let parts: Uint8Array[] = [];
  let hasVcl = false;
  let key = false;
  let hasSps = false;
  const finish = () => {
    if (!hasVcl) return;
    const size = parts.reduce((n, p) => n + 4 + p.length, 0);
    const data = new Uint8Array(size);
    let o = 0;
    for (const p of parts) {
      data.set([0, 0, 0, 1], o);
      data.set(p, o + 4);
      o += 4 + p.length;
    }
    out.push({ data, key, hasSps });
    parts = [];
    hasVcl = key = hasSps = false;
  };
  for (const nal of splitNals(stream)) {
    const t = nalType(nal);
    const vcl = t >= NAL.SLICE && t <= NAL.IDR;
    if (hasVcl) {
      const isParam = t === NAL.SPS || t === NAL.PPS || t === NAL.AUD || t === NAL.SEI;
      if (isParam || (vcl && nal.length > 1 && (nal[1] & 0x80) !== 0)) finish();
    }
    parts.push(nal);
    hasVcl ||= vcl;
    key ||= t === NAL.IDR;
    hasSps ||= t === NAL.SPS;
  }
  finish();
  return out;
}
