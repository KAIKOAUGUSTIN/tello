//! CRCs do protocolo do app: CRC-8 (Maxim, semente 0x77) no cabeçalho e
//! CRC-16 (Kermit, semente 0x3692) no pacote inteiro.

const fn table8() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u8;
        let mut b = 0;
        while b < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0x8c } else { c >> 1 };
            b += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const fn table16() -> [u16; 256] {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut b = 0;
        while b < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0x8408 } else { c >> 1 };
            b += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static TABLE8: [u8; 256] = table8();
static TABLE16: [u16; 256] = table16();

pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0x77, |crc, &b| TABLE8[(crc ^ b) as usize])
}

pub fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0x3692, |crc, &b| TABLE16[((crc ^ b as u16) & 0xff) as usize] ^ (crc >> 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_known_entries() {
        assert_eq!(&TABLE8[..4], &[0x00, 0x5e, 0xbc, 0xe2]);
        assert_eq!(&TABLE16[..4], &[0x0000, 0x1189, 0x2312, 0x329b]);
    }

    #[test]
    fn known_takeoff_packet() {
        // Pacote de decolagem publicado na documentação da comunidade:
        // cc 58 00 7c 68 54 00 e4 01 c2 16
        let pkt = [0xcc, 0x58, 0x00, 0x7c, 0x68, 0x54, 0x00, 0xe4, 0x01, 0xc2, 0x16];
        assert_eq!(crc8(&pkt[..3]), 0x7c);
        assert_eq!(crc16(&pkt[..9]), u16::from_le_bytes([0xc2, 0x16]));
    }
}
