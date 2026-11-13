//! Reads yt-dlp's output. The app asks for three marked lines, the video's details as JSON, a progress line per
//! chunk and the final path, and also watches for errors and post-processing.

use std::path::PathBuf;

use serde::Deserialize;

/// The lines the app asks yt-dlp to print start with this.
pub(super) const MARK: &str = "NB|";
pub(super) const META: &str = "NB|M|%(.{id,title,uploader,duration,thumbnail,filesize_approx})j";
pub(super) const STEP: &str = "download:NB|P|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s";
pub(super) const FILE: &str = "after_move:NB|F|%(filepath)s";
/// Post-processing that follows the download. While it runs, a long silence isn't a stall.
const MERGING: [&str; 6] = ["[Merger]", "[ExtractAudio]", "[VideoRemuxer]", "[Metadata]", "[Fixup", "[MoveFiles]"];
const ERROR: &str = "ERROR:";

/// The video's details, printed before the download starts.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub filesize_approx: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Line {
    Meta(Meta),
    Step(Step),
    Merging,
    File(PathBuf),
    Error(String),
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Step {
    pub(super) finished: bool,
    pub(super) received: u64,
    pub(super) total: Option<u64>,
    pub(super) speed: Option<f64>,
    pub(super) eta: Option<f64>,
}

pub(super) fn parse(line: &str) -> Line {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix(MARK) {
        return marked(rest).unwrap_or(Line::Other);
    }
    if let Some(problem) = line.trim_start().strip_prefix(ERROR) {
        return Line::Error(problem.trim().to_owned());
    }
    if MERGING.iter().any(|stage| line.starts_with(stage)) { Line::Merging } else { Line::Other }
}

fn marked(rest: &str) -> Option<Line> {
    let (kind, body) = rest.split_once('|')?;
    match kind {
        "M" => serde_json::from_str(body).ok().map(Line::Meta),
        "F" => (!body.is_empty()).then(|| Line::File(PathBuf::from(body))),
        "P" => {
            let mut fields = body.split('|');
            let status = fields.next()?;
            let mut number = || fields.next().and_then(|field| field.trim().parse::<f64>().ok());
            let (received, total, estimate, speed, eta) = (number(), number(), number(), number(), number());
            Some(Line::Step(Step {
                finished: status == "finished",
                received: received.unwrap_or(0.0).max(0.0) as u64,
                total: total.or(estimate).filter(|total| *total > 0.0).map(|total| total as u64),
                speed: speed.filter(|speed| *speed > 0.0),
                eta: eta.filter(|eta| *eta >= 0.0),
            }))
        }
        _ => None,
    }
}

/// Adds up the files of one download, picture and sound, into one share that never runs backwards.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Tally {
    whole: Option<u64>,
    landed: u64,
    current: u64,
    current_total: Option<u64>,
    share: f32,
}

impl Tally {
    /// The size yt-dlp guessed for the whole download, if it gave one.
    pub(super) fn expect(&mut self, whole: Option<f64>) {
        self.whole = whole.filter(|whole| *whole > 0.0).map(|whole| whole as u64);
    }

    pub(super) fn step(&mut self, step: Step) -> f32 {
        if step.finished {
            self.landed += step.total.unwrap_or(step.received).max(step.received);
            (self.current, self.current_total) = (0, None);
        } else {
            (self.current, self.current_total) = (step.received, step.total);
        }
        let have = self.landed + self.current;
        let known = self.landed + self.current_total.unwrap_or(self.current);
        // An estimate that turns out too small grows to what's known to be coming.
        let whole = self.whole.map_or(known, |whole| whole.max(known));
        let share = if whole == 0 { 0.0 } else { (have as f64 / whole as f64).min(1.0) as f32 };
        self.share = self.share.max(share);
        self.share
    }

    pub(super) fn received(&self) -> u64 {
        self.landed + self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(line: &str) -> Step {
        match parse(line) {
            Line::Step(step) => step,
            other => panic!("{line} read as {other:?}"),
        }
    }

    #[test]
    fn real_ytdlp_lines_are_understood() {
        let meta = r#"NB|M|{"id": "jNQXAC9IVRw", "title": "Me at the zoo | 4K", "uploader": "jawed", "duration": 19, "thumbnail": "https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg", "filesize_approx": 742369}"#;
        let Line::Meta(meta) = parse(meta) else { panic!("meta") };
        assert_eq!(
            (meta.title.as_str(), meta.duration, meta.filesize_approx),
            ("Me at the zoo | 4K", Some(19.0), Some(742_369.0))
        );
        assert_eq!(meta.uploader.as_deref(), Some("jawed"));

        let moving = step("NB|P|downloading|130048|433081|NA|5478494.702824314|0");
        assert_eq!((moving.finished, moving.received, moving.total), (false, 130_048, Some(433_081)));
        assert_eq!((moving.speed.map(f64::round), moving.eta), (Some(5_478_495.0), Some(0.0)));
        let estimated = step("NB|P|downloading|1024|NA|900000.5|NA|NA");
        assert_eq!((estimated.total, estimated.speed, estimated.eta), (Some(900_000), None, None));
        assert!(step("NB|P|finished|433081|433081|NA|2372174.1|NA").finished);

        assert_eq!(
            parse("NB|F|/Users/me/Downloads/Me at the zoo.mp4\r\n"),
            Line::File("/Users/me/Downloads/Me at the zoo.mp4".into())
        );
        assert_eq!(parse(r#"[Merger] Merging formats into "/tmp/a.mp4""#), Line::Merging);
        assert_eq!(
            parse("ERROR: [youtube] xxxxxxxxxxx: This video is unavailable"),
            Line::Error("[youtube] xxxxxxxxxxx: This video is unavailable".into())
        );
        assert_eq!(parse("[youtube] jNQXAC9IVRw: Downloading webpage"), Line::Other);
        assert_eq!(parse("NB|M|not json"), Line::Other);
        assert_eq!(parse("NB|P|"), Line::Step(Step::default()));
    }

    #[test]
    fn partial_metadata_still_reads() {
        let Line::Meta(meta) = parse(r#"NB|M|{"id": "DAbc", "title": "Video by someone", "duration": null}"#) else {
            panic!("meta")
        };
        assert_eq!((meta.id.as_str(), meta.duration, meta.thumbnail, meta.filesize_approx), ("DAbc", None, None, None));
    }

    #[test]
    fn picture_and_sound_fill_one_bar() {
        let mut tally = Tally::default();
        tally.expect(Some(742_369.0));
        let quarter = tally.step(step("NB|P|downloading|185592|433081|NA|1|1"));
        assert!((quarter - 0.25).abs() < 0.01, "{quarter}");
        let picture = tally.step(step("NB|P|finished|433081|433081|NA|1|NA"));
        assert!((picture - 0.583).abs() < 0.01, "{picture}");
        let sound = tally.step(step("NB|P|downloading|1024|309288|NA|1|1"));
        assert!(sound >= picture, "the sound starting must not pull the bar back");
        assert_eq!(tally.step(step("NB|P|finished|309288|309288|NA|1|NA")), 1.0);
        assert_eq!(tally.received(), 742_369);
    }

    #[test]
    fn unknown_size_is_measured_per_file() {
        let mut tally = Tally::default();
        tally.expect(None);
        assert_eq!(tally.step(step("NB|P|downloading|50|100|NA|1|1")), 0.5);
        assert_eq!(tally.step(step("NB|P|finished|100|100|NA|1|NA")), 1.0);
        assert_eq!(tally.step(step("NB|P|downloading|10|100|NA|1|1")), 1.0, "it holds rather than jumping back");
        assert_eq!(tally.step(step("NB|P|downloading|10|NA|NA|NA|NA")), 1.0);
    }

    #[test]
    fn small_estimate_grows_with_arrivals() {
        let mut tally = Tally::default();
        tally.expect(Some(100.0));
        assert_eq!(tally.step(step("NB|P|downloading|150|300|NA|1|1")), 0.5);
    }
}
