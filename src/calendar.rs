use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use ureq::Agent;

use crate::platform::PlatformEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub summary: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub location: Option<String>,
}

pub enum CalendarCommand {
    SetUrl(String),
    Refresh,
}

pub fn parse_ics(ics: &str) -> Vec<CalendarEvent> {
    let unfolded = unfold_ics_lines(ics);
    let mut events = Vec::new();
    let mut in_event = false;
    let mut summary = String::new();
    let mut location: Option<String> = None;
    let mut start_utc: Option<i64> = None;
    let mut end_utc: Option<i64> = None;

    for line in unfolded {
        match line.as_str() {
            "BEGIN:VEVENT" => {
                in_event = true;
                summary.clear();
                location = None;
                start_utc = None;
                end_utc = None;
            }
            "END:VEVENT" => {
                if in_event && let Some(start) = start_utc {
                    let end = end_utc.unwrap_or(start);
                    let title = if summary.trim().is_empty() {
                        "(No title)".to_owned()
                    } else {
                        summary.trim().to_owned()
                    };
                    events.push(CalendarEvent {
                        summary: title,
                        start_utc: start,
                        end_utc: end,
                        location: location.take().filter(|l| !l.trim().is_empty()),
                    });
                }
                in_event = false;
            }
            _ if in_event => {
                let Some((key_part, val)) = line.split_once(':') else {
                    continue;
                };
                let key = key_part.split(';').next().unwrap_or(key_part);
                match key {
                    "SUMMARY" => summary = unescape_ics(val),
                    "LOCATION" => location = Some(unescape_ics(val)),
                    "DTSTART" => start_utc = parse_ics_datetime(val.trim()),
                    "DTEND" => end_utc = parse_ics_datetime(val.trim()),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    events
}

fn unfold_ics_lines(ics: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in ics.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(cont) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(prev) = out.last_mut() {
                prev.push_str(cont);
            }
        } else if !line.is_empty() {
            out.push(line.to_owned());
        }
    }
    out
}

fn unescape_ics(s: &str) -> String {
    s.replace("\\n", " ")
        .replace("\\N", " ")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn parse_ics_datetime(val: &str) -> Option<i64> {
    if let Ok(ndt) = NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%SZ") {
        return Some(ndt.and_utc().timestamp());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%S") {
        let local_ts = Local
            .from_local_datetime(&ndt)
            .single()
            .map(|dt| dt.timestamp());
        return Some(local_ts.unwrap_or_else(|| ndt.and_utc().timestamp()));
    }
    if let Ok(date) = NaiveDate::parse_from_str(val, "%Y%m%d")
        && let Some(ndt) = date.and_hms_opt(0, 0, 0)
    {
        let local_ts = Local
            .from_local_datetime(&ndt)
            .single()
            .map(|dt| dt.timestamp());
        return Some(local_ts.unwrap_or_else(|| ndt.and_utc().timestamp()));
    }
    None
}

pub fn upcoming_events(
    events: &[CalendarEvent],
    now_utc: i64,
    limit: usize,
) -> Vec<&CalendarEvent> {
    let mut upcoming: Vec<&CalendarEvent> = events
        .iter()
        .filter(|ev| {
            if ev.end_utc > ev.start_utc {
                ev.end_utc >= now_utc
            } else {
                ev.start_utc >= now_utc - 600
            }
        })
        .collect();
    upcoming.sort_by_key(|ev| ev.start_utc);
    upcoming.truncate(limit);
    upcoming
}

pub fn format_countdown(start_utc: i64, end_utc: i64, now_utc: i64) -> String {
    if start_utc <= now_utc && now_utc <= end_utc.max(start_utc + 600) {
        return "Now".to_owned();
    }
    let diff = (start_utc - now_utc).max(0);
    let mins = diff / 60;
    if mins < 60 {
        format!("In {}m", mins.max(1))
    } else if mins < 60 * 24 {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 {
            format!("In {h}h")
        } else {
            format!("In {h}h {m}m")
        }
    } else {
        format!("In {}d", mins / (60 * 24))
    }
}

pub fn format_local_time(utc_ts: i64) -> String {
    DateTime::from_timestamp(utc_ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_owned())
}

pub fn spawn_calendar_worker(
    initial_url: String,
    cmd_rx: mpsc::Receiver<CalendarCommand>,
    event_tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("calendar-worker".to_owned())
        .spawn(move || {
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .build();
            let agent: Agent = config.into();
            let mut url = initial_url.trim().to_owned();

            loop {
                if !url.is_empty() {
                    let res = fetch_ics(&agent, &url);
                    let _ = event_tx.send(PlatformEvent::CalendarUpdated(res));
                    on_wake();
                } else {
                    let _ = event_tx.send(PlatformEvent::CalendarUpdated(Ok(Vec::new())));
                    on_wake();
                }

                match cmd_rx.recv_timeout(Duration::from_secs(300)) {
                    Ok(CalendarCommand::SetUrl(next)) => {
                        url = next.trim().to_owned();
                    }
                    Ok(CalendarCommand::Refresh) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn calendar worker thread");
}

fn fetch_ics(agent: &Agent, url: &str) -> Result<Vec<CalendarEvent>, String> {
    const MAX_ICS_BYTES: u64 = 2 * 1024 * 1024;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Invalid URL (must start with https:// or http://)".to_owned());
    }
    let body = agent
        .get(url)
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .call()
        .map_err(|e| format!("Sync error: {e}"))?
        .body_mut()
        .with_config()
        .limit(MAX_ICS_BYTES)
        .read_to_string()
        .map_err(|e| format!("Read error: {e}"))?;
    let now_utc = chrono::Utc::now().timestamp();
    let min_ts = now_utc - 86_400;
    let max_ts = now_utc + 90 * 86_400;
    let mut events = parse_ics(&body);
    events.retain(|ev| ev.end_utc.max(ev.start_utc) >= min_ts && ev.start_utc <= max_ts);
    events.sort_by_key(|ev| ev.start_utc);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ics_unfolds_and_unescapes() {
        let sample = "\
BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\n\
DTSTART:20261125T150000Z\r\n\
DTEND:20261125T160000Z\r\n\
SUMMARY:Sprint\\, Review \r\n Part 2\r\n\
LOCATION:Room A\\; Meet\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let events = parse_ics(sample);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Sprint, Review Part 2");
        assert_eq!(events[0].location.as_deref(), Some("Room A; Meet"));
        assert!(events[0].end_utc > events[0].start_utc);
    }

    #[test]
    fn upcoming_events_filters_and_formats_countdown() {
        let events = vec![
            CalendarEvent {
                summary: "Past".to_owned(),
                start_utc: 100,
                end_utc: 200,
                location: None,
            },
            CalendarEvent {
                summary: "Later".to_owned(),
                start_utc: 5000,
                end_utc: 6000,
                location: None,
            },
            CalendarEvent {
                summary: "Soon".to_owned(),
                start_utc: 1300,
                end_utc: 2000,
                location: None,
            },
        ];

        let next = upcoming_events(&events, 1000, 2);
        assert_eq!(next.len(), 2);
        assert_eq!(next[0].summary, "Soon");
        assert_eq!(next[1].summary, "Later");
        assert_eq!(format_countdown(1300, 2000, 1000), "In 5m");
        assert_eq!(format_countdown(900, 2000, 1000), "Now");
    }
}
