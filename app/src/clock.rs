//! Czas lokalny bez zależności: przesunięcie strefy, data `YYYY-MM-DD`,
//! godzina i znacznik do nazwy pliku.
//!
//! Runda 17: prompt systemowy musi znać dzisiejszą datę (model nie ma zegara),
//! a `/export` nazywa plik datą i godziną lokalną. Jedno źródło prawdy dla
//! obu miejsc — strefę bierzemy z `date +%z`, jak robił to pasek zadania.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Przesunięcie strefy lokalnej od UTC w sekundach (`+0200` → 7200).
/// Raz na proces: strefa w trakcie pracy nie zmienia się w sposób, który
/// miały sens śledzić, a `date` w podprocesie nie może iść na każdą klatkę.
pub fn offset_secs() -> i64 {
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let out = match std::process::Command::new("date").arg("+%z").output() {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => return 0,
        };
        parse_zone(&out).unwrap_or(0)
    })
}

/// `+0200` / `-0730` → sekundy od UTC.
pub fn parse_zone(raw: &str) -> Option<i64> {
    let sign = match raw.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    let hours: i64 = digits.get(0..2)?.parse().ok()?;
    let minutes: i64 = digits.get(2..4)?.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// Sekundy uniksowe; 0 zamiast paniki, gdy zegar systemowy stoi przed epoką.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `HH:MM` w czasie lokalnym z sekund uniksowych.
pub fn clock(secs: u64) -> String {
    let day = local_day_secs(secs);
    format!("{:02}:{:02}", day / 3600, (day % 3600) / 60)
}

fn local_day_secs(secs: u64) -> i64 {
    (secs as i64 + offset_secs()).rem_euclid(86_400)
}

/// `(rok, miesiąc, dzień)` w czasie lokalnym z sekund uniksowych.
pub fn date_of(secs: u64) -> (i32, u32, u32) {
    let days = (secs as i64 + offset_secs()).div_euclid(86_400);
    civil_from_days(days)
}

/// Dzisiejsza data lokalna `YYYY-MM-DD` — ta sama, którą widzi człowiek
/// w `date +%F`.
pub fn today() -> String {
    let (y, m, d) = date_of(now_secs());
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Pas strefy do promptu: `UTC+02:00`, `UTC-07:30`, `UTC+00:00` dla zera.
pub fn zone_label() -> String {
    let offset = offset_secs();
    let sign = if offset < 0 { '-' } else { '+' };
    let abs = offset.abs();
    format!("UTC{}{:02}:{:02}", sign, abs / 3600, (abs % 3600) / 60)
}

/// Linia do promptu systemowego: `Data: 2026-11-14 (UTC+02:00)`.
pub fn date_line() -> String {
    format!("Data: {} ({})", today(), zone_label())
}

/// `YYYYMMDD-HHMM` — domyślna nazwa eksportu.
pub fn stamp_compact() -> String {
    let secs = now_secs();
    let (y, m, d) = date_of(secs);
    let day = local_day_secs(secs);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}",
        y,
        m,
        d,
        day / 3600,
        (day % 3600) / 60
    )
}

/// `YYYY-MM-DD HH:MM` — nagłówek eksportu.
pub fn date_time() -> String {
    let secs = now_secs();
    let (y, m, d) = date_of(secs);
    format!("{:04}-{:02}-{:02} {}", y, m, d, clock(secs))
}

/// Dni od 1970-01-01 → data cywilna (algorytm Hinnanta; bez `chrono`).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    ((if m <= 2 { y + 1 } else { y }) as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_labels_and_offsets_round_trip() {
        assert_eq!(parse_zone("+0200"), Some(7200));
        assert_eq!(parse_zone("-0730"), Some(-27_000));
        assert_eq!(parse_zone("+0000"), Some(0));
        assert_eq!(parse_zone("0200"), None);
        // `parse_zone` czyta to, co wypisuje `date +%z` w tej strefie.
        let raw = std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if let Some(raw) = raw {
            assert_eq!(parse_zone(&raw), Some(offset_secs()));
        }
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_000), (2022, 1, 8));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }

    #[test]
    fn today_is_the_same_date_as_the_system_reports() {
        let out = std::process::Command::new("date").arg("+%F").output();
        if let Ok(out) = out {
            if out.status.success() {
                let expected = String::from_utf8_lossy(&out.stdout).trim().to_string();
                assert_eq!(today(), expected, "data lokalna musi zgadzac sie z `date +%F`");
            }
        }
        assert!(stamp_compact().starts_with(&today().replace('-', "")));
        assert_eq!(date_line(), format!("Data: {} ({})", today(), zone_label()));
        assert!(zone_label().starts_with("UTC"));
    }
}
