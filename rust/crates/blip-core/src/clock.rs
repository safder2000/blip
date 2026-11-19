//! Reading and showing times: "25", "1:30", "90s", "5pm", "tomorrow 10", "in 20m"
//! (`blip/timers.py` and `blip/notes.py`). Local time, weekdays Monday = 0.

use chrono::{Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Timelike};

pub const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
pub const WEEKDAYS: [u32; 5] = [0, 1, 2, 3, 4];
pub const EVERYDAY: [u32; 7] = [0, 1, 2, 3, 4, 5, 6];

/// MM:SS or H:MM:SS. Countdowns round up, stopwatches (up = false) round down.
pub fn fmt_clock(secs: f64, up: bool) -> String {
    let secs = if up { (secs - 1e-6).ceil() } else { secs.floor() }.max(0.0) as i64;
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// '25' -> 25 min, '1:30' -> 1 h 30 min, '90s', '2h', '1h 20m', '45 min' -> seconds.
pub fn parse_duration(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if let Ok(n) = t.parse::<f64>() {
        let s = (n * 60.0) as i64;
        return (s > 0).then_some(s as f64);
    }
    let parts: Vec<&str> = t.split(':').collect();
    if (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        let n: Vec<i64> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
        if parts[1].len() > 2 || parts.get(2).map_or(false, |p| p.len() > 2) {
            return None;
        }
        let s = if n.len() == 3 { n[0] * 3600 + n[1] * 60 + n[2] } else { n[0] * 3600 + n[1] * 60 };
        return (s > 0).then_some(s as f64);
    }
    // "1h 20m", "90s", "45 min"
    let mut total = 0.0;
    let mut rest = t.as_str();
    let mut any = false;
    while !rest.trim().is_empty() {
        rest = rest.trim_start();
        let num_len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        if num_len == 0 {
            return None;
        }
        let n: f64 = rest[..num_len].parse().ok()?;
        rest = rest[num_len..].trim_start();
        let unit_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        let unit = &rest[..unit_len];
        let mult = match unit {
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            _ => return None,
        };
        total += n * mult;
        rest = &rest[unit_len..];
        any = true;
    }
    let s = total as i64;
    (any && s > 0).then_some(s as f64)
}

/// '9:30', '09:30', '9.30', '930', '5pm', '5:15 pm' -> (h, m).
pub fn parse_clock(text: &str) -> Option<(u32, u32)> {
    let t: String = text.trim().to_lowercase().chars().filter(|c| *c != ' ').collect();
    let (body, ap) = if let Some(b) = t.strip_suffix("am") {
        (b.to_string(), Some(false))
    } else if let Some(b) = t.strip_suffix("pm") {
        (b.to_string(), Some(true))
    } else {
        (t.clone(), None)
    };
    let (h, m): (u32, u32) = if let Some((a, b)) = body.split_once([':', '.']) {
        if a.is_empty() || a.len() > 2 || b.len() != 2 {
            return None;
        }
        (a.parse().ok()?, b.parse().ok()?)
    } else if body.chars().all(|c| c.is_ascii_digit()) && (1..=4).contains(&body.len()) {
        if body.len() <= 2 {
            (body.parse().ok()?, 0)
        } else {
            let (a, b) = body.split_at(body.len() - 2);
            (a.parse().ok()?, b.parse().ok()?)
        }
    } else {
        return None;
    };
    let h = match ap {
        Some(pm) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            h % 12 + if pm { 12 } else { 0 }
        }
        None => h,
    };
    (h < 24 && m < 60).then_some((h, m))
}

fn local_ts(d: NaiveDate, h: u32, m: u32) -> Option<f64> {
    match Local.from_local_datetime(&d.and_hms_opt(h, m, 0)?) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => Some(t.timestamp() as f64),
        LocalResult::None => None,
    }
}

fn local_date(ts: f64) -> NaiveDate {
    Local.timestamp_opt(ts as i64, 0).single().map(|t| t.date_naive()).unwrap_or_default()
}

/// Next time after `now` at hh:mm on one of `days` (weekday numbers, Monday = 0; empty = any day).
pub fn next_fire(hh: u32, mm: u32, days: &[u32], now: f64) -> Option<f64> {
    let base = local_date(now);
    for add in 0..8 {
        let d = base + Duration::days(add);
        let ts = local_ts(d, hh, mm)?;
        if ts > now && (days.is_empty() || days.contains(&d.weekday().num_days_from_monday())) {
            return Some(ts);
        }
    }
    None
}

pub fn describe_days(days: &[u32]) -> String {
    let mut s = days.to_vec();
    s.sort_unstable();
    if s.is_empty() {
        "once".into()
    } else if s == EVERYDAY {
        "every day".into()
    } else if s == WEEKDAYS {
        "weekdays".into()
    } else {
        s.iter().map(|d| DAY_NAMES[*d as usize % 7]).collect::<Vec<_>>().join(", ")
    }
}

/// When a reminder should go off, from what people type ('in 20m', '5pm', 'tomorrow 10'...).
pub fn parse_when(text: &str, now: f64) -> Option<f64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    let today = local_date(now);
    for (word, add) in [("today", 0), ("tomorrow", 1), ("tmrw", 1), ("tmr", 1)] {
        if let Some(rest) = t.strip_prefix(word) {
            let rest = rest.trim();
            let rest = rest.strip_prefix("at").map(str::trim).unwrap_or(rest);
            let (h, m) = if rest.is_empty() { (9, 0) } else { parse_clock(rest)? };
            let ts = local_ts(today + Duration::days(add), h, m)?;
            return (ts > now).then_some(ts);
        }
    }
    if let Some(rest) = t.strip_prefix("in ") {
        return parse_duration(rest).map(|s| now + s);
    }
    let clockish = t.starts_with("at ")
        || t.ends_with("am")
        || t.ends_with("pm")
        || (t.contains([':', '.']) && t.split([':', '.']).all(|p| p.chars().all(|c| c.is_ascii_digit())) && t.len() <= 5);
    if clockish {
        let (h, m) = parse_clock(t.strip_prefix("at ").unwrap_or(&t))?;
        let ts = local_ts(today, h, m)?;
        return Some(if ts > now { ts } else { local_ts(today + Duration::days(1), h, m)? });
    }
    parse_duration(&t).map(|s| now + s)
}

/// 'in 25 min', 'today 17:30', 'tomorrow 09:00', 'Mon 24 Nov 09:00'.
pub fn describe_when(ts: f64, now: f64) -> String {
    let left = ts - now;
    if (0.0..3600.0).contains(&left) {
        return format!("in {} min", ((left / 60.0).round() as i64).max(1));
    }
    let Some(d) = Local.timestamp_opt(ts as i64, 0).single() else { return String::new() };
    let hm = format!("{:02}:{:02}", d.hour(), d.minute());
    let days = (d.date_naive() - local_date(now)).num_days();
    match days {
        0 => format!("today {hm}"),
        1 => format!("tomorrow {hm}"),
        -1 => format!("yesterday {hm}"),
        _ => format!("{} {hm}", d.format("%a %d %b")),
    }
}
