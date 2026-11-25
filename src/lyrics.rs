use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde::Deserialize;
use ureq::Agent;

use crate::platform::PlatformEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsData {
    Loading,
    Synced(Vec<LyricLine>),
    Plain(Vec<String>),
    NotFound,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibResponse {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw_line in lrc.lines() {
        let mut rest = raw_line.trim();
        let mut timestamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end_idx) = stripped.find(']') else {
                break;
            };
            let tag = &stripped[..end_idx];
            if let Some(ms) = parse_lrc_timestamp(tag) {
                timestamps.push(ms);
            }
            rest = stripped[end_idx + 1..].trim_start();
        }
        let text = rest.trim();
        if text.is_empty() || timestamps.is_empty() {
            continue;
        }
        for time_ms in timestamps {
            out.push(LyricLine {
                time_ms,
                text: text.to_owned(),
            });
        }
    }
    out.sort_by_key(|line| line.time_ms);
    out
}

fn parse_lrc_timestamp(tag: &str) -> Option<u64> {
    let (mins_str, sec_part) = tag.split_once(':')?;
    let mins: u64 = mins_str.trim().parse().ok()?;
    let (secs_str, frac_str) = match sec_part.split_once('.') {
        Some((s, f)) => (s, f),
        None => (sec_part, "0"),
    };
    let secs: u64 = secs_str.trim().parse().ok()?;
    let frac_digits = frac_str.trim();
    let ms: u64 = match frac_digits.len() {
        0 => 0,
        1 => frac_digits.parse::<u64>().ok()? * 100,
        2 => frac_digits.parse::<u64>().ok()? * 10,
        _ => frac_digits[..3].parse::<u64>().ok()?,
    };
    Some((mins * 60 + secs) * 1000 + ms)
}

pub fn active_lyric_line(data: &LyricsData, position_ms: u64) -> Option<(&str, Option<&str>)> {
    match data {
        LyricsData::Synced(lines) if !lines.is_empty() => {
            let next_idx = lines.partition_point(|l| l.time_ms <= position_ms);
            if next_idx == 0 {
                return Some((lines[0].text.as_str(), lines.get(1).map(|l| l.text.as_str())));
            }
            let curr = &lines[next_idx - 1];
            let next = lines.get(next_idx).map(|l| l.text.as_str());
            Some((curr.text.as_str(), next))
        }
        LyricsData::Plain(lines) => lines
            .first()
            .map(|first| (first.as_str(), lines.get(1).map(String::as_str))),
        _ => None,
    }
}

pub fn spawn_lyrics_worker(
    req_rx: mpsc::Receiver<(String, String)>,
    event_tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("lyrics-worker".to_owned())
        .spawn(move || {
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build();
            let agent: Agent = config.into();
            let mut cache: HashMap<(String, String), LyricsData> = HashMap::new();

            while let Ok((mut artist, mut title)) = req_rx.recv() {
                // Drain queued track switches so rapid skips only fetch the newest track.
                while let Ok(newer) = req_rx.try_recv() {
                    artist = newer.0;
                    title = newer.1;
                }

                let key = (artist.clone(), title.clone());
                let lyrics = if let Some(cached) = cache.get(&key) {
                    cached.clone()
                } else {
                    let fetched = fetch_lyrics(&agent, &artist, &title);
                    cache.insert(key.clone(), fetched.clone());
                    fetched
                };

                let _ = event_tx.send(PlatformEvent::LyricsLoaded {
                    artist,
                    title,
                    lyrics,
                });
                on_wake();
            }
        })
        .expect("spawn lyrics worker thread");
}

fn fetch_lyrics(agent: &Agent, artist: &str, title: &str) -> LyricsData {
    let clean_title = strip_title_noise(title);
    if let Some(data) = query_lrclib_get(agent, artist, &clean_title) {
        return data;
    }
    query_lrclib_search(agent, artist, &clean_title).unwrap_or(LyricsData::NotFound)
}

fn query_lrclib_get(agent: &Agent, artist: &str, title: &str) -> Option<LyricsData> {
    if artist.trim().is_empty() || title.trim().is_empty() {
        return None;
    }
    let body = agent
        .get("https://lrclib.net/api/get")
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .query("artist_name", artist)
        .query("track_name", title)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let parsed: LrcLibResponse = serde_json::from_str(&body).ok()?;
    response_to_lyrics(parsed)
}

fn query_lrclib_search(agent: &Agent, artist: &str, title: &str) -> Option<LyricsData> {
    let q = format!("{artist} {title}").trim().to_owned();
    if q.is_empty() {
        return None;
    }
    let body = agent
        .get("https://lrclib.net/api/search")
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .query("q", &q)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let list: Vec<LrcLibResponse> = serde_json::from_str(&body).ok()?;
    list.into_iter().find_map(response_to_lyrics)
}

fn response_to_lyrics(resp: LrcLibResponse) -> Option<LyricsData> {
    if let Some(synced) = resp.synced_lyrics {
        let lines = parse_lrc(&synced);
        if !lines.is_empty() {
            return Some(LyricsData::Synced(lines));
        }
    }
    if let Some(plain) = resp.plain_lyrics {
        let lines: Vec<String> = plain
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if !lines.is_empty() {
            return Some(LyricsData::Plain(lines));
        }
    }
    None
}

fn strip_title_noise(title: &str) -> String {
    let mut s = title.trim().to_owned();
    for marker in [" (Official", " [Official", " - Remastered", " (Remastered"] {
        if let Some(idx) = s.find(marker) {
            s.truncate(idx);
        }
    }
    s.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lrc_handles_multi_tags_and_sorts() {
        let lrc = "\
[ar:Artist]
[00:12.50][00:30.000]Chorus line
[00:05.2]Intro line
[00:08.00]
";
        let lines = parse_lrc(lrc);
        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time_ms: 5200,
                    text: "Intro line".to_owned(),
                },
                LyricLine {
                    time_ms: 12500,
                    text: "Chorus line".to_owned(),
                },
                LyricLine {
                    time_ms: 30000,
                    text: "Chorus line".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn active_lyric_tracks_playback_position() {
        let data = LyricsData::Synced(vec![
            LyricLine {
                time_ms: 5000,
                text: "First".to_owned(),
            },
            LyricLine {
                time_ms: 10000,
                text: "Second".to_owned(),
            },
        ]);
        assert_eq!(active_lyric_line(&data, 1000), Some(("First", Some("Second"))));
        assert_eq!(active_lyric_line(&data, 7000), Some(("First", Some("Second"))));
        assert_eq!(active_lyric_line(&data, 10500), Some(("Second", None)));
    }
}
