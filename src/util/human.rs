//! Human friendly formatting helpers.

use std::time::Duration;

/// Format a byte count as `1.2 MB` / `842 B`.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Format a duration as `420 ms`, `1.8 s` or `2m 04s`.
pub fn duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{:.0} ms", d.as_millis())
    } else if secs < 60.0 {
        format!("{secs:.2} s")
    } else {
        let total = d.as_secs();
        format!("{}m {:02}s", total / 60, total % 60)
    }
}

/// Shorten a string to at most `max` columns, appending an ellipsis.
pub fn truncate(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut width = 0usize;
    for ch in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push('…');
    out
}

/// Pad a string to `width` display columns (right padding).
pub fn pad_right(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    let w = UnicodeWidthStr::width(s);
    if w >= width {
        truncate(s, width)
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

/// Convert a number to a roman-ish pretty percentage: `42%`.
pub fn percent(fraction: f32) -> String {
    format!("{:>3}%", (fraction.clamp(0.0, 1.0) * 100.0).round() as u32)
}

/// Simplistic plural helper: `1 file`, `3 files`.
pub fn files(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

/// Format Unix seconds as `2026-11-16 12:34 UTC` (no external deps).
pub fn unix_datetime(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

#[cfg(test)]
mod date_tests {
    use super::unix_datetime;

    #[test]
    fn epoch_is_1970() {
        assert_eq!(unix_datetime(0), "1970-01-01 00:00 UTC");
        assert_eq!(unix_datetime(1_700_000_000), "2023-11-14 22:13 UTC");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_formats() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KB");
        assert_eq!(bytes(1536), "1.50 KB");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello w…");
    }
}
