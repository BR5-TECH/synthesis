//! Instants, as this module reads and writes them
//! (`../../../specifications/core/DSS-draft-statistics-storage.md` DSS-FR-MHJC).
//!
//! An event carries its capture instant as an RFC 3339 UTC timestamp, because a
//! log is a committed text file an author reads with a file manager and an
//! epoch number is not something a person reads. The fold needs milliseconds to
//! subtract two of them, so this is where the one is turned into the other.
//!
//! Parsing here rather than through a date crate because the only shape this
//! module ever meets is the one `crate::notes::format_rfc3339_millis_utc`
//! writes — a fixed-width UTC stamp with an optional millisecond fraction — and
//! a line that carries anything else is a damaged line the fold skips
//! (DSS-FR-LDFK) rather than a format to accommodate.

/// The instant `text` names, in milliseconds since the Unix epoch, or `None`
/// where it is not a fixed-width UTC stamp this module wrote.
///
/// Accepts `YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS.mmmZ`, with a
/// fraction of any length; the fraction is read to millisecond precision and
/// anything finer is dropped rather than rounded, so two instants one
/// microsecond apart order by the rest of the stamp instead of by noise.
pub fn parse_instant_ms(text: &str) -> Option<i64> {
    let body = text.strip_suffix('Z').or_else(|| text.strip_suffix('z'))?;
    let (date, rest) = body.split_once('T').or_else(|| body.split_once('t'))?;
    let (clock, fraction) = match rest.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (rest, None),
    };

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let millis = match fraction {
        None => 0,
        Some(digits) => {
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let mut value: i64 = 0;
            for index in 0..3 {
                let digit = digits.as_bytes().get(index).map(|b| (b - b'0') as i64).unwrap_or(0);
                value = value * 10 + digit;
            }
            value
        }
    };

    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1_000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian calendar date.
///
/// Howard Hinnant's `days_from_civil`, which is exact for every year this
/// application can meet and needs no table.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
