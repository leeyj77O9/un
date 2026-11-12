//! `Date` support: the reference stores a .NET `DateTime`, whose tick unit is
//! 100 ns from 0001-01-01. A `Date` payload is that tick count, so arithmetic on
//! dates is arithmetic on ticks.

pub const TICKS_PER_DAY: i64 = 864_000_000_000;
pub const TICKS_PER_MS: i64 = 10_000;
/// 100 ns ticks, so a second is 10 ms ticks = 10_000_000. `TICKS_PER_DAY` is
/// `86_400 * TICKS_PER_SEC`, and `ticks_from_civil` uses `ss * 10_000_000`, so the
/// whole date model is built on this scale.
pub const TICKS_PER_SEC: i64 = 10_000_000;

/// .NET `DateTime.DaysInMonth`
pub fn days_in_month(year: i64, month: i64) -> i64 {
    let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let m = month as usize;
    if m == 2 && is_leap_year(year) {
        29
    } else {
        days[m - 1]
    }
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// `civil_from_days` — Howard Hinnant's algorithm, the same date model .NET uses
/// for the proleptic Gregorian calendar.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// ticks to the unix-epoch day number used by `civil_from_days` (days from
/// 1970-01-01, floored).
pub fn ticks_to_days(ticks: i64) -> i64 {
    (ticks - unix_epoch_ticks()).div_euclid(TICKS_PER_DAY)
}

pub fn parts(ticks: i64) -> (i64, i64, i64, i64, i64, i64, i64) {    let (y, m, d) = civil_from_days(ticks_to_days(ticks));
    let time = ticks.rem_euclid(TICKS_PER_DAY);
    let hour = time / 36_000_000_000;
    let minute = (time % 36_000_000_000) / 600_000_000;
    let second = (time % 600_000_000) / 10_000_000;
    let ms = (time % 10_000_000) / 10_000;
    (y, m, d, hour, minute, second, ms)
}

/// `Date.ToStr()` — `"yyyy-MM-dd HH:mm:ss.fff"`
pub fn format(ticks: i64) -> String {
    let (y, m, d, hh, mm, ss, ms) = parts(ticks);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}.{ms:03}")
}

/// `Date + Str` uses `"yyyy-MM-dd"`
pub fn format_date(ticks: i64) -> String {
    let (y, m, d, _, _, _, _) = parts(ticks);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `DateTime.ToString(format)` — the custom format specifiers of .NET: `y` (the year),
/// `M` (the month), `d` (the day), `H` (the hour), `m` (the minute), `s` (the second) and
/// `f` (the fraction of a second). a run of the same specifier is the padded width.
pub fn format_pattern(ticks: i64, format: &str) -> String {
    let (year, month, day, hour, minute, second, ms) = parts(ticks);

    let mut out = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let specifier = chars[i];
        let mut width = 1;
        while i + width < chars.len() && chars[i + width] == specifier {
            width += 1;
        }

        match specifier {
            'y' => out.push_str(&pad(year, width.min(4))),
            'M' => out.push_str(&pad(month, width.min(2))),
            'd' => out.push_str(&pad(day, width.min(2))),
            'H' => out.push_str(&pad(hour, width.min(2))),
            'm' => out.push_str(&pad(minute, width.min(2))),
            's' => out.push_str(&pad(second, width.min(2))),
            'f' => {
                // the fraction of a second, the digits are the tenths, hundredths and
                // thousandths
                let digits = width.min(3);
                let value = ms / 10i64.pow(3 - digits as u32);
                out.push_str(&pad(value, digits));
            }
            _ => {
                for _ in 0..width {
                    out.push(specifier);
                }
            }
        }

        i += width;
    }

    out
}

/// the number padded with zeros, the .NET format specifiers pad to the width.
fn pad(value: i64, width: usize) -> String {
    let text = value.to_string();
    if text.len() >= width {
        return text;
    }
    format!("{}{}", "0".repeat(width - text.len()), text)
}

pub fn year(ticks: i64) -> i64 {
    civil_from_days(ticks_to_days(ticks)).0
}

pub fn month(ticks: i64) -> i64 {
    civil_from_days(ticks_to_days(ticks)).1
}

pub fn day(ticks: i64) -> i64 {
    civil_from_days(ticks_to_days(ticks)).2
}

pub fn hour(ticks: i64) -> i64 {
    ticks.rem_euclid(TICKS_PER_DAY) / 36_000_000_000
}

pub fn minute(ticks: i64) -> i64 {
    (ticks.rem_euclid(TICKS_PER_DAY) % 36_000_000_000) / 600_000_000
}

pub fn second(ticks: i64) -> i64 {
    (ticks.rem_euclid(TICKS_PER_DAY) % 600_000_000) / 10_000_000
}

pub fn ms(ticks: i64) -> i64 {
    (ticks.rem_euclid(TICKS_PER_DAY) % 10_000_000) / 10_000
}

pub fn weekday(ticks: i64) -> i64 {
    // .NET DayOfWeek: Sunday = 0, and 1970-01-01 (day 0) was a Thursday.
    (ticks_to_days(ticks) + 4).rem_euclid(7)
}

pub fn day_of_year(ticks: i64) -> i64 {
    let (y, m, d) = civil_from_days(ticks_to_days(ticks));
    let mut total = d;
    for k in 1..m {
        total += days_in_month(y, k);
    }
    total
}

/// `new DateTimeOffset(self.Value).ToUnixTimeSeconds()` — .NET maps
/// `0001-01-01` to the unix epoch offset of 62135596800 s.
/// the ticks of the unix epoch, the .NET ticks count from 0001-01-01.
pub fn unix_epoch_ticks() -> i64 {
    621_355_968_000_000_000
}

/// `DateTime.TryParse` — the forms the tests use: `yyyy-MM-dd` and
/// `yyyy-MM-ddTHH:mm:ss` (a space is accepted as the separator too).
pub fn parse(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date_part, time_part) = match text.split_once(['T', ' ']) {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };

    let mut date_fields = date_part.split(['-', '/']);
    let year: i64 = date_fields.next()?.parse().ok()?;
    let month: i64 = date_fields.next()?.parse().ok()?;
    let day: i64 = date_fields.next()?.parse().ok()?;

    if date_fields.next().is_some() {
        return None;
    }

    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }

    let (mut hour, mut minute, mut second, mut ms) = (0i64, 0i64, 0i64, 0i64);

    if let Some(time) = time_part {
        let mut fields = time.split(':');
        hour = fields.next()?.parse().ok()?;
        minute = fields.next().unwrap_or("0").parse().ok()?;
        second = fields.next().unwrap_or("0").parse().ok()?;

        if let Some(fraction) = fields.next() {
            let mut digits = fraction.chars();
            let mut value = 0i64;
            let mut count = 0;
            for c in digits.by_ref().take(3) {
                let digit = c.to_digit(10)? as i64;
                value = value * 10 + digit;
                count += 1;
            }
            ms = value * 10i64.pow(3 - count as u32);
        }
    }

    Some(ticks_from_civil(year, month, day, hour, minute, second, ms))
}

pub fn timestamp(ticks: i64) -> i64 {
    (ticks - unix_epoch_ticks()).div_euclid(TICKS_PER_SEC)
}

pub fn timestamp_ms(ticks: i64) -> i64 {
    (ticks - unix_epoch_ticks()).div_euclid(TICKS_PER_MS)
}

/// the inverse of `civil_from_days`: day number from 1970-01-01
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// builds ticks from date parts, for tests and for the `date` natives
pub fn ticks_from_civil(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64, ms: i64) -> i64 {
    days_from_civil(y, m, d) * TICKS_PER_DAY + unix_epoch_ticks()
        + hh * 36_000_000_000
        + mm * 600_000_000
        + ss * 10_000_000
        + ms * TICKS_PER_MS
}

/// UTF-16 code unit at `index` of a string (the reference indexes `string` by
/// `char`, which is a UTF-16 code unit). A lone surrogate has no `char`
/// counterpart, so it decodes to the replacement character.
pub fn utf16_char(s: &str, index: usize) -> String {
    let unit = s.encode_utf16().nth(index).unwrap_or(0);
    String::from_utf16_lossy(&[unit])
}

pub fn utf16_join(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_display_matches_dotnet_pattern() {
        let t = ticks_from_civil(2024, 1, 5, 10, 20, 30, 123);
        assert_eq!(format(t), "2024-01-05 10:20:30.123");
        assert_eq!(format_date(t), "2024-01-05");
        assert_eq!(year(t), 2024);
        assert_eq!(month(t), 1);
        assert_eq!(day(t), 5);
        assert_eq!(hour(t), 10);
        assert_eq!(minute(t), 20);
        assert_eq!(second(t), 30);
        assert_eq!(ms(t), 123);
    }

    #[test]
    fn unix_epoch_boundary() {
        let t = ticks_from_civil(1970, 1, 1, 0, 0, 0, 0);
        assert_eq!(timestamp(t), 0);
        assert_eq!(timestamp_ms(t), 0);
        assert_eq!(format(t), "1970-01-01 00:00:00.000");
        // .NET `DateTime.MinValue` is 0001-01-01, and 1970-01-01 is day 719162
        assert_eq!(days_from_civil(1, 1, 1) + 719162, 0);
        assert_eq!(format(0), "0001-01-01 00:00:00.000");
    }

    #[test]
    fn civil_round_trip() {
        for (y, m, d) in [(1, 1, 1), (1970, 1, 1), (2000, 2, 29), (2024, 12, 31), (2026, 11, 11)] {
            let t = ticks_from_civil(y, m, d, 0, 0, 0, 0);
            assert_eq!(parts(t), (y, m, d, 0, 0, 0, 0), "{y}-{m}-{d}");
        }
    }

    #[test]
    fn calendar_helpers_match_dotnet() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
        // 2024-01-05 was a Friday, and .NET's DayOfWeek starts at Sunday = 0
        assert_eq!(weekday(ticks_from_civil(2024, 1, 5, 0, 0, 0, 0)), 5);
        assert_eq!(day_of_year(ticks_from_civil(2024, 1, 5, 0, 0, 0, 0)), 5);
        assert_eq!(day_of_year(ticks_from_civil(2024, 3, 1, 0, 0, 0, 0)), 61);
    }
}
