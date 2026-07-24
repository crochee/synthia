use chrono::{DateTime, Datelike, TimeZone, Timelike};

pub(super) fn add_months<Tz: TimeZone>(
    dt: DateTime<Tz>,
    months: u32,
) -> DateTime<Tz> {
    let year = dt.year();
    let month = dt.month();
    let day = dt.day();

    let total_months = (year * 12 + month as i32 - 1) + months as i32;
    let new_year = total_months / 12;
    let new_month = (total_months % 12 + 1) as u32;

    let max_day = days_in_month(new_year, new_month);
    let new_day = day.min(max_day);

    dt.timezone()
        .with_ymd_and_hms(
            new_year,
            new_month,
            new_day,
            dt.hour(),
            dt.minute(),
            dt.second(),
        )
        .single()
        .unwrap_or(dt)
}

pub(super) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 31,
    }
}
