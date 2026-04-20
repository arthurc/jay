//! Focused native/JDK shims for library behavior that is too broad to
//! interpret from OpenJDK bytecode yet.

use crate::{JayError, JayResult};

const MILLIS_PER_SECOND: i64 = 1_000;
const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const MILLIS_PER_HOUR: i64 = 60 * MILLIS_PER_MINUTE;
const MILLIS_PER_DAY: i64 = 24 * MILLIS_PER_HOUR;

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Debug, Clone, Copy)]
struct UtcDateTime {
    year: i32,
    month: u8,
    day: u8,
    weekday: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

pub(super) fn date_to_string(epoch_millis: i64) -> String {
    let date_time = utc_date_time(epoch_millis);
    format!(
        "{} {} {} {}:{}:{} GMT {}",
        WEEKDAYS[date_time.weekday as usize],
        MONTHS[date_time.month as usize - 1],
        two_digits(date_time.day),
        two_digits(date_time.hour),
        two_digits(date_time.minute),
        two_digits(date_time.second),
        date_time.year
    )
}

pub(super) fn format_simple_date(pattern: &str, epoch_millis: i64) -> JayResult<String> {
    if pattern != "hh.mm aa" {
        return Err(JayError::new(format!(
            "unsupported SimpleDateFormat pattern {pattern}"
        )));
    }

    let date_time = utc_date_time(epoch_millis);
    let mut hour = date_time.hour % 12;
    if hour == 0 {
        hour = 12;
    }
    let marker = if date_time.hour < 12 { "AM" } else { "PM" };

    Ok(format!(
        "{}.{} {marker}",
        two_digits(hour),
        two_digits(date_time.minute)
    ))
}

fn utc_date_time(epoch_millis: i64) -> UtcDateTime {
    let days = epoch_millis.div_euclid(MILLIS_PER_DAY);
    let millis_of_day = epoch_millis.rem_euclid(MILLIS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = (millis_of_day / MILLIS_PER_HOUR) as u8;
    let minute = ((millis_of_day % MILLIS_PER_HOUR) / MILLIS_PER_MINUTE) as u8;
    let second = ((millis_of_day % MILLIS_PER_MINUTE) / MILLIS_PER_SECOND) as u8;
    let weekday = (days + 4).rem_euclid(7) as u8;

    UtcDateTime {
        year,
        month,
        day,
        weekday,
        hour,
        minute,
        second,
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u8, u8) {
    let days = days_since_epoch + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year as i32, month as u8, day as u8)
}

fn two_digits(value: u8) -> String {
    format!("{value:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch_date_to_jdk_style_gmt_string() {
        assert_eq!(date_to_string(0), "Thu Jan 01 00:00:00 GMT 1970");
    }

    #[test]
    fn formats_test2_time_pattern_in_utc() {
        assert_eq!(format_simple_date("hh.mm aa", 0).unwrap(), "12.00 AM");
        assert_eq!(
            format_simple_date("hh.mm aa", 13 * MILLIS_PER_HOUR + 5 * MILLIS_PER_MINUTE).unwrap(),
            "01.05 PM"
        );
    }
}
