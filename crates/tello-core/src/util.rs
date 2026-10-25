//! Data e hora (UTC) sem dependências externas.

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

pub fn now_utc() -> DateTime {
    from_unix_millis(unix_millis())
}

/// Milissegundos desde 1970 (UTC).
pub fn unix_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

pub fn from_unix_millis(ms: i64) -> DateTime {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Algoritmo "civil from days" (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    DateTime {
        year: yoe + era * 400 + i64::from(month <= 2),
        month: month as u8,
        day: day as u8,
        hour: (rem / 3600) as u8,
        minute: (rem % 3600 / 60) as u8,
        second: (rem % 60) as u8,
        millis: ms.rem_euclid(1000) as u16,
    }
}

/// Carimbo para nomes de arquivo: AAAAMMDD-HHMMSS (UTC).
pub fn utc_stamp() -> String {
    let t = now_utc();
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_dates() {
        let t = from_unix_millis(0);
        assert_eq!((t.year, t.month, t.day, t.hour), (1970, 1, 1, 0));
        // 2024-02-29 12:34:56.789 UTC
        let t = from_unix_millis(1_709_210_096_789);
        assert_eq!((t.year, t.month, t.day), (2024, 2, 29));
        assert_eq!((t.hour, t.minute, t.second, t.millis), (12, 34, 56, 789));
    }
}
