//! Small text helpers for the UI, with no dependency on a date library.

/// `YYYY-MM-DD` (UTC) from Unix seconds. Uses the standard "civil from days" algorithm.
pub fn ymd(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC) from Unix seconds.
pub fn iso_utc(unix_secs: i64) -> String {
    let secs_of_day = unix_secs.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        ymd(unix_secs),
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// `45 MB/s`.
pub fn speed(bytes_per_sec: f64) -> String {
    format!(
        "{}/s",
        rl_core::library::format_bytes(bytes_per_sec.max(0.0) as u64)
    )
}

/// `about 6 min left`, or an empty string when the speed is not known yet.
pub fn eta(remaining_bytes: u64, bytes_per_sec: f64) -> String {
    if bytes_per_sec < 1.0 {
        return String::new();
    }
    let secs = (remaining_bytes as f64 / bytes_per_sec) as u64;
    match secs {
        0..=59 => "less than a minute left".to_string(),
        60..=5_399 => format!("about {} min left", secs.div_ceil(60)),
        _ => format!("about {:.1} h left", secs as f64 / 3600.0),
    }
}

/// The date part of an ISO 8601 string (`2026-11-08T...` gives `2026-11-08`); empty if it does not look like one.
pub fn iso_date(iso: &str) -> String {
    let b = iso.as_bytes();
    let ok = b.len() >= 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit);
    if ok {
        iso[..10].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ymd_known_dates() {
        assert_eq!(ymd(0), "1970-01-01");
        assert_eq!(ymd(951_782_400), "2000-02-29"); // leap day
        assert_eq!(ymd(1_794_000_000), "2026-11-06");
        assert_eq!(ymd(-86_400), "1969-12-31");
    }

    #[test]
    fn iso_utc_is_formatted_like_the_sidecar_dates() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_794_000_123), "2026-11-06T21:22:03Z");
    }

    #[test]
    fn speed_and_eta_read_naturally() {
        assert_eq!(speed(45.0 * 1024.0 * 1024.0), "45.0 MB/s");
        assert_eq!(eta(100, 0.0), "");
        assert_eq!(
            eta(10 * 1024 * 1024, 10.0 * 1024.0 * 1024.0),
            "less than a minute left"
        );
        assert_eq!(
            eta(21 * 1024 * 1024 * 1024, 46.0 * 1024.0 * 1024.0),
            "about 8 min left"
        );
        assert_eq!(
            eta(100 * 1024 * 1024 * 1024, 5.0 * 1024.0 * 1024.0),
            "about 5.7 h left"
        );
    }

    #[test]
    fn iso_date_only_accepts_real_dates() {
        assert_eq!(iso_date("2025-01-02T03:04:05.000Z"), "2025-01-02");
        assert_eq!(iso_date("06-12-2025"), "");
        assert_eq!(iso_date(""), "");
        assert_eq!(iso_date("2025/01/02"), "");
    }
}
