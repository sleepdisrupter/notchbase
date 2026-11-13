//! The Mac players notchbase follows, Spotify and Music, and the AppleScript that asks and controls each.

use crate::media::applescript::SEPARATOR;
use crate::media::track::Control;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Artwork {
    Url,
    Raw,
}

pub struct Player {
    pub bundle: &'static str,
    pub notification: &'static str,
    pub artwork: Artwork,
    /// Spotify gives durations in milliseconds, Music in seconds.
    pub seconds_per_duration_unit: f64,
}

pub const PLAYERS: [Player; 2] = [
    Player {
        bundle: "com.spotify.client",
        notification: "com.spotify.client.PlaybackStateChanged",
        artwork: Artwork::Url,
        seconds_per_duration_unit: 0.001,
    },
    Player {
        bundle: "com.apple.Music",
        notification: "com.apple.Music.playerInfo",
        artwork: Artwork::Raw,
        seconds_per_duration_unit: 1.0,
    },
];

impl Player {
    pub fn now_script(&self) -> String {
        let sep = u32::from(SEPARATOR);
        let field = |expression: &str| format!("({expression} as text)");
        let fields = [
            field("player state"),
            field("name of t"),
            field("artist of t"),
            field("album of t"),
            field("player position"),
            field("duration of t"),
        ];
        format!(
            "tell application id \"{}\"\nset t to current track\nreturn {}\nend tell",
            self.bundle,
            fields.join(&format!(" & (character id {sep}) & "))
        )
    }

    pub fn artwork_script(&self) -> String {
        let query = match self.artwork {
            Artwork::Url => "artwork url of current track",
            Artwork::Raw => "raw data of artwork 1 of current track",
        };
        format!("tell application id \"{}\" to return {query}", self.bundle)
    }

    pub fn control_script(&self, control: Control) -> String {
        let command = match control {
            Control::PlayPause => "playpause".to_owned(),
            Control::Next => "next track".to_owned(),
            Control::Previous => "previous track".to_owned(),
            Control::Seek(seconds) => format!("set player position to {:.2}", seconds.max(0.0)),
        };
        format!("tell application id \"{}\" to {command}", self.bundle)
    }
}
