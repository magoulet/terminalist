//! Date and time utility functions
//!
//! This module provides functions for date manipulation and human-readable formatting,
//! similar to how Todoist displays dates (e.g., "yesterday", "today", "tomorrow").

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Weekday};

/// Standard date format used throughout the application for Todoist API compatibility
pub const TODOIST_DATE_FORMAT: &str = "%Y-%m-%d";

/// Parse a date string in YYYY-MM-DD format to NaiveDate
///
/// # Arguments
/// * `date_str` - Date string in YYYY-MM-DD format
///
/// # Returns
/// * `Result<NaiveDate, chrono::ParseError>` - Parsed date or parse error
pub fn parse_date(date_str: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(date_str, TODOIST_DATE_FORMAT)
}

/// Format a NaiveDate to YYYY-MM-DD string
pub fn format_ymd(d: NaiveDate) -> String {
    d.format(TODOIST_DATE_FORMAT).to_string()
}

/// Format current local date to YYYY-MM-DD string
pub fn format_today() -> String {
    format_ymd(Local::now().date_naive())
}

/// Format date with offset from today to YYYY-MM-DD string
///
/// # Arguments
/// * `days_offset` - Number of days to add/subtract from today
///
/// # Returns
/// * `String` - Date string in YYYY-MM-DD format
pub fn format_date_with_offset(days_offset: i64) -> String {
    let target_date = Local::now().date_naive() + Duration::days(days_offset);
    format_ymd(target_date)
}

/// Calculate the next occurrence of a target weekday from a given date
pub fn next_weekday(from: NaiveDate, target: Weekday) -> NaiveDate {
    let from_w = from.weekday().num_days_from_monday() as i64;
    let tgt_w = target.num_days_from_monday() as i64;
    let mut delta = (7 + tgt_w - from_w) % 7;
    if delta == 0 {
        delta = 7;
    }
    from + Duration::days(delta)
}

/// Format a date string in Todoist-style human-readable format
///
/// # Arguments
/// * `date_str` - Date string in YYYY-MM-DD format
///
/// # Returns
/// * `String` - Human-readable date format
pub fn format_human_date(date_str: &str) -> String {
    // Parse the input date string
    let input_date = match parse_date(date_str) {
        Ok(date) => date,
        Err(_) => return date_str.to_string(), // Return original if parsing fails
    };

    // Get current local date
    let now = Local::now();
    let today = now.date_naive();

    // Calculate the difference in days
    let days_diff = (input_date - today).num_days();

    match days_diff {
        -1 => "yesterday".to_string(),
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        diff if diff > 1 && diff <= 7 => {
            // Within the next week - show day name
            format!("next {}", input_date.format("%A"))
        }
        diff if (-7..-1).contains(&diff) => {
            // Within the past week - show day name
            format!("last {}", input_date.format("%A"))
        }
        diff if diff > 7 && diff <= 30 => {
            // Within the next month - show "in X days"
            format!("in {} days", diff)
        }
        diff if (-30..-7).contains(&diff) => {
            // Within the past month - show "X days ago"
            format!("{} days ago", -diff)
        }
        _ => {
            // For dates further out, show the actual date
            // Format as "Jan 15" or "Jan 15, 2025" if different year
            let current_year = today.year();
            let input_year = input_date.year();

            if input_year == current_year {
                input_date.format("%b %d").to_string()
            } else {
                input_date.format("%b %d, %Y").to_string()
            }
        }
    }
}

/// Format a datetime string in Todoist-style human-readable format
///
/// # Arguments
/// * `datetime_str` - DateTime string in various formats (RFC3339, ISO 8601, etc.)
///
/// # Returns
/// * `String` - Human-readable datetime format
pub fn format_human_datetime(datetime_str: &str) -> String {
    // Try multiple datetime parsing strategies
    let parsed_dt = if let Ok(dt) = DateTime::parse_from_rfc3339(datetime_str) {
        // RFC3339 with timezone (e.g., "2025-01-15T14:30:00Z")
        Some(dt.with_timezone(&Local))
    } else if let Ok(dt) =
        chrono::NaiveDateTime::parse_from_str(datetime_str, &format!("{}T%H:%M:%S%.f", TODOIST_DATE_FORMAT))
    {
        // ISO 8601 without timezone (e.g., "2025-01-15T14:30:00")
        Some(
            Local
                .from_local_datetime(&dt)
                .single()
                .unwrap_or_else(|| Local.from_utc_datetime(&dt)),
        )
    } else if let Ok(dt) =
        chrono::NaiveDateTime::parse_from_str(datetime_str, &format!("{} %H:%M:%S", TODOIST_DATE_FORMAT))
    {
        // Space-separated format (e.g., "2025-01-15 14:30:00")
        Some(
            Local
                .from_local_datetime(&dt)
                .single()
                .unwrap_or_else(|| Local.from_utc_datetime(&dt)),
        )
    } else {
        None
    };

    if let Some(local_dt) = parsed_dt {
        let date_str = local_dt.format(TODOIST_DATE_FORMAT).to_string();
        let time_str = local_dt.format("%H:%M").to_string();

        let human_date = format_human_date(&date_str);

        // Always show time for datetime strings
        format!("{} at {}", human_date, time_str)
    } else {
        // Fallback to date-only parsing
        format_human_date(datetime_str)
    }
}

/// Splits a Todoist due date into the plain day the views filter on and, for a task with a
/// time, the full date and time for display.
///
/// Todoist puts the time inside `date` for timed tasks ("2026-11-20T13:00:00", or with a
/// trailing "Z" for a fixed timezone) and may leave `datetime` empty. The views compare
/// `due_date` with "YYYY-MM-DD" strings, so it must hold the day alone. A fixed-timezone time
/// is in UTC, so its day is taken in local time, where it can differ.
#[must_use]
pub fn split_todoist_due(date: &str, datetime: Option<&str>) -> (String, Option<String>) {
    if !date.contains('T') {
        return (date.to_string(), datetime.map(str::to_string));
    }
    let full = datetime.unwrap_or(date);
    let day = match DateTime::parse_from_rfc3339(full) {
        Ok(dt) => dt.with_timezone(&Local).format(TODOIST_DATE_FORMAT).to_string(),
        Err(_) => date.chars().take(10).collect(),
    };
    (day, Some(full.to_string()))
}

/// The start of `day` and of the day after, in local time, as RFC 3339 times in UTC. This is
/// the range Todoist's completed-tasks call takes for "everything completed on `day`".
#[must_use]
pub fn local_day_bounds_utc(day: NaiveDate) -> (String, String) {
    let to_utc = |date: NaiveDate| {
        let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
        Local
            .from_local_datetime(&midnight)
            .earliest()
            .unwrap_or_else(|| Local.from_utc_datetime(&midnight))
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    };
    (to_utc(day), to_utc(day + Duration::days(1)))
}

/// Reads a day typed by the user, relative to `today`: "today", "yesterday", "tomorrow",
/// "3 days ago", "-3", a weekday ("monday", "mon": the latest one on or before today),
/// "2026-11-02", or a month and day ("nov 2", "2 november": the latest one on or before
/// today). Returns `None` for anything else.
#[must_use]
pub fn parse_day(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let text = input.trim().to_lowercase();
    let words: Vec<&str> = text.split_whitespace().collect();

    match words.as_slice() {
        ["today"] => return Some(today),
        ["yesterday"] => return Some(today - Duration::days(1)),
        ["tomorrow"] => return Some(today + Duration::days(1)),
        [n, "day" | "days", "ago"] => return n.parse::<i64>().ok().map(|n| today - Duration::days(n)),
        [n] if n.starts_with('-') => return n[1..].parse::<i64>().ok().map(|n| today - Duration::days(n)),
        _ => {}
    }

    if let Ok(date) = NaiveDate::parse_from_str(&text, TODOIST_DATE_FORMAT) {
        return Some(date);
    }

    if let [word] = words.as_slice() {
        if let Ok(weekday) = word.parse::<Weekday>() {
            let back = (today.weekday().num_days_from_monday() + 7 - weekday.num_days_from_monday()) % 7;
            return Some(today - Duration::days(i64::from(back)));
        }
    }

    // "nov 2" or "2 nov", in the latest year that doesn't put it after today.
    let (month, day) = match words.as_slice() {
        [a, b] => match (month_number(a), b.parse::<u32>(), month_number(b), a.parse::<u32>()) {
            (Some(month), Ok(day), _, _) | (_, _, Some(month), Ok(day)) => (month, day),
            _ => return None,
        },
        _ => return None,
    };
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
    if this_year <= today {
        Some(this_year)
    } else {
        NaiveDate::from_ymd_opt(today.year() - 1, month, day)
    }
}

/// Month number for an English month name or its first three letters.
fn month_number(word: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    if word.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|month| month.starts_with(word))
        .map(|index| index as u32 + 1)
}
