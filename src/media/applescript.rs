//! Reads what the Mac players' AppleScript answers: a line of track fields, and artwork as a `«data ...»` blob.

use std::time::Instant;

use super::track::{Timeline, Track};

/// The ASCII unit separator the scripts put between fields.
pub const SEPARATOR: char = '\u{1f}';
const DATA_OPEN: &str = "«data ";
const DATA_CLOSE: char = '»';
const TYPE_CODE: usize = 4;

/// One line of state, title, artist, album, position and duration. Durations are scaled by
/// `seconds_per_duration_unit`, since Spotify counts in milliseconds.
pub fn parse_track(text: &str, seconds_per_duration_unit: f64) -> Option<Track> {
    let mut fields = text.trim_end_matches(['\r', '\n']).split(SEPARATOR);
    let (state, title) = (fields.next()?.trim().to_ascii_lowercase(), fields.next()?.trim());
    if state.is_empty() || state == "stopped" || title.is_empty() {
        return None;
    }
    let (artist, album) = (fields.next().unwrap_or_default(), fields.next().unwrap_or_default());
    let position = fields.next().and_then(number);
    let duration = fields.next().and_then(number).map(|d| d * seconds_per_duration_unit);
    Some(Track {
        title: title.to_owned(),
        artist: artist.trim().to_owned(),
        playing: state == "playing",
        artwork_key: album.trim().to_owned(),
        timeline: position.zip(duration).and_then(|(p, d)| Timeline::new(p, d, Instant::now())),
    })
}

/// The bytes of a `«data ...»` blob, after its four-letter type code.
pub fn parse_data(text: &str) -> Option<Vec<u8>> {
    let start = text.find(DATA_OPEN)? + DATA_OPEN.len();
    let body = text[start..].split(DATA_CLOSE).next()?;
    let hex = body.get(TYPE_CODE..)?;
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

fn number(text: &str) -> Option<f64> {
    // AppleScript writes numbers in the user's locale, so the decimal point can be a comma.
    text.trim().replace(',', ".").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_output_becomes_a_track() {
        let line = format!("playing{SEPARATOR}Song{SEPARATOR}Band{SEPARATOR}Record\n");
        let track = parse_track(&line, 1.0).expect("a track");
        assert_eq!((track.title.as_str(), track.artist.as_str(), track.playing), ("Song", "Band", true));
        assert_eq!(track.artwork_key, "Record");
        assert!(track.timeline.is_none());
        let paused = parse_track(&format!("Paused{SEPARATOR}Song"), 1.0).expect("a track");
        assert!(!paused.playing && paused.artist.is_empty());
        assert!(parse_track(&format!("stopped{SEPARATOR}Song"), 1.0).is_none());
        assert!(parse_track(&format!("playing{SEPARATOR}  "), 1.0).is_none());
        assert!(parse_track("", 1.0).is_none());
    }

    #[test]
    fn script_output_carries_the_timeline() {
        let line = format!("playing{SEPARATOR}Song{SEPARATOR}Band{SEPARATOR}Record{SEPARATOR}12,5{SEPARATOR}200000");
        let timeline = parse_track(&line, 0.001).and_then(|t| t.timeline).expect("a timeline");
        assert_eq!((timeline.position, timeline.duration), (12.5, 200.0));
        let no_length = format!("playing{SEPARATOR}Song{SEPARATOR}Band{SEPARATOR}Record{SEPARATOR}3{SEPARATOR}0");
        assert!(parse_track(&no_length, 1.0).and_then(|t| t.timeline).is_none());
    }

    #[test]
    fn raw_artwork_is_decoded_from_hex() {
        assert_eq!(parse_data("«data tdtaFFD8FF00»"), Some(vec![0xFF, 0xD8, 0xFF, 0x00]));
        assert_eq!(parse_data("«data JPEG0A0b»\n"), Some(vec![0x0A, 0x0B]));
        assert_eq!(parse_data("«data tdtaF»"), None);
        assert_eq!(parse_data("missing value"), None);
    }
}
