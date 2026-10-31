//! The last good board from a live feed, kept on disk: so a restart shows
//! something at once, and with no network the board shows what it last
//! knew, its footer saying when that was.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::model::{Direction, Mode, Movement, Place, Status};

#[derive(Serialize, Deserialize)]
struct Saved {
    at: DateTime<Utc>,
    tz: String,
    movements: Vec<SavedMovement>,
    notices: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct SavedMovement {
    id: String,
    mode: Mode,
    direction: Direction,
    scheduled: DateTime<Utc>,
    expected: Option<DateTime<Utc>>,
    numbers: Vec<String>,
    operator: String,
    place: String,
    via: Vec<String>,
    stand: Option<String>,
    terminal: Option<String>,
    status: Status,
}

/// Where the cache lives: the system's cache folder, `now-boarding` in it.
fn dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    };
    base.map(|b| b.join("now-boarding"))
}

/// The file for a board: the feed, the place and the direction, made safe
/// for a file name.
fn file(dir: &std::path::Path, feed: &str, place: &Place, direction: Direction) -> PathBuf {
    let name: String = format!("{feed}-{}-{direction:?}", place.id)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{name}.json"))
}

/// Saves a board, when it was got, and its notices. Failing to save
/// matters little: the board just starts empty next time.
pub fn save(
    feed: &str,
    place: &Place,
    direction: Direction,
    at: DateTime<Utc>,
    movements: &[Movement],
    notices: &[String],
) {
    if let Some(d) = dir() {
        let _ = save_in(&d, feed, place, direction, at, movements, notices);
    }
}

fn save_in(
    d: &std::path::Path,
    feed: &str,
    place: &Place,
    direction: Direction,
    at: DateTime<Utc>,
    movements: &[Movement],
    notices: &[String],
) -> std::io::Result<()> {
    std::fs::create_dir_all(d)?;
    let saved = Saved {
        at,
        tz: place.tz.clone(),
        movements: movements
            .iter()
            .map(|m| SavedMovement {
                id: m.id.clone(),
                mode: m.mode,
                direction: m.direction,
                scheduled: m.scheduled.with_timezone(&Utc),
                expected: m.expected.map(|e| e.with_timezone(&Utc)),
                numbers: m.numbers.clone(),
                operator: m.operator.clone(),
                place: m.place.clone(),
                via: m.via.clone(),
                stand: m.stand.clone(),
                terminal: m.terminal.clone(),
                status: m.status,
            })
            .collect(),
        notices: notices.to_vec(),
    };
    let path = file(d, feed, place, direction);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(&saved)?)?;
    std::fs::rename(tmp, path)
}

/// The board last saved, when it was got, and its notices.
pub fn load(
    feed: &str,
    place: &Place,
    direction: Direction,
) -> Option<(DateTime<Utc>, Vec<Movement>, Vec<String>)> {
    load_in(&dir()?, feed, place, direction)
}

fn load_in(
    d: &std::path::Path,
    feed: &str,
    place: &Place,
    direction: Direction,
) -> Option<(DateTime<Utc>, Vec<Movement>, Vec<String>)> {
    let bytes = std::fs::read(file(d, feed, place, direction)).ok()?;
    let s: Saved = serde_json::from_slice(&bytes).ok()?;
    let tz: Tz = s.tz.parse().unwrap_or(Tz::UTC);
    let ms = s
        .movements
        .into_iter()
        .map(|m| Movement {
            id: m.id,
            mode: m.mode,
            direction: m.direction,
            scheduled: m.scheduled.with_timezone(&tz),
            expected: m.expected.map(|e| e.with_timezone(&tz)),
            numbers: m.numbers,
            operator: m.operator,
            place: m.place,
            via: m.via,
            stand: m.stand,
            terminal: m.terminal,
            status: m.status,
        })
        .collect();
    Some((s.at, ms, s.notices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn a_board_comes_back_as_it_was_saved() {
        let d = std::env::temp_dir().join(format!("now-boarding-test-{}", std::process::id()));
        let place = Place {
            id: "ptv:0:1181,3:1544".into(),
            name: "Southern Cross".into(),
            tz: "Australia/Melbourne".into(),
        };
        let tz: Tz = "Australia/Melbourne".parse().unwrap();
        let m = Movement {
            id: "0:948001:1".into(),
            mode: Mode::Train,
            direction: Direction::Departure,
            scheduled: tz.with_ymd_and_hms(2026, 10, 30, 18, 5, 0).unwrap(),
            expected: None,
            numbers: Vec::new(),
            operator: "METRO".into(),
            place: "Frankston".into(),
            via: vec!["Richmond".into()],
            stand: Some("12".into()),
            terminal: None,
            status: Status::OnTime,
        };
        let at = Utc.with_ymd_and_hms(2026, 10, 30, 7, 0, 0).unwrap();
        save_in(
            &d,
            "ptv",
            &place,
            Direction::Departure,
            at,
            std::slice::from_ref(&m),
            &["Works".into()],
        )
        .unwrap();
        let (got_at, ms, notices) = load_in(&d, "ptv", &place, Direction::Departure).unwrap();
        assert_eq!(
            (got_at, ms, notices),
            (at, vec![m], vec!["Works".to_string()])
        );
        // Another board, another file.
        assert!(load_in(&d, "ptv", &place, Direction::Arrival).is_none());
        let _ = std::fs::remove_dir_all(d);
    }
}
