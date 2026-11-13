//! Matches the progress other apps publish to the partial file it describes, even when the browser salted the name.

use crate::system::downloads::scan::{Listing, finished_name};

const SALT: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Published {
    pub(super) name: String,
    pub(super) total: Option<u64>,
}

impl Published {
    fn tells_of(&self, partial: &str) -> bool {
        partial == self.name || finished_name(partial).is_some_and(|stem| stem == self.name || self.salted(stem))
    }

    fn salted(&self, stem: &str) -> bool {
        let (base, extensions) = self.name.split_at(self.name.find('.').unwrap_or(self.name.len()));
        let between = stem.strip_prefix(base).and_then(|rest| rest.strip_suffix(extensions));
        let salt = between.and_then(|salt| salt.strip_prefix('.'));
        salt.is_some_and(|salt| {
            salt.len() == SALT && salt.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
        })
    }
}

pub(super) fn fill(listing: &mut Listing, published: &[Published]) {
    for partial in &mut listing.partials {
        let Some(found) = published.iter().find(|published| published.tells_of(&partial.name)) else { continue };
        if found.name != partial.name {
            partial.lands_as = Some(found.name.clone());
        }
        partial.total = partial.total.or(found.total.filter(|total| *total >= partial.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::downloads::scan::Partial;

    fn published(name: &str, total: u64) -> Published {
        Published { name: name.into(), total: Some(total) }
    }

    fn filled(partial: &str, size: u64, published: &[Published]) -> Partial {
        let partial = Partial { name: partial.into(), size, total: None, lands_as: None };
        let mut listing = Listing { partials: vec![partial], ..Listing::default() };
        fill(&mut listing, published);
        listing.partials.remove(0)
    }

    #[test]
    fn published_progress_gives_total() {
        let chrome = filled("movie.mp4.crdownload", 40, &[published("movie.mp4.crdownload", 100)]);
        assert_eq!((chrome.total, chrome.lands_as), (Some(100), None));
        let plain = filled("movie.mp4.part", 40, &[published("movie.mp4", 100)]);
        assert_eq!((plain.total, plain.lands_as.as_deref()), (Some(100), Some("movie.mp4")));
        let other = filled("song.mp3.part", 40, &[published("movie.mp4", 100)]);
        assert_eq!((other.total, other.lands_as), (None, None));
    }

    #[test]
    fn firefox_salts_unfinished_names() {
        let movie = [published("movie.mp4", 100)];
        let salted = filled("movie.Xy3_Q9-b.mp4.part", 40, &movie);
        assert_eq!((salted.total, salted.lands_as.as_deref()), (Some(100), Some("movie.mp4")));
        assert_eq!(filled("movie.Xy3kQ9.mp4.part", 40, &movie).total, None, "a salt is eight characters");
        assert_eq!(filled("movie.dir.cuts.mp4.part", 40, &movie).total, None, "a salt has no dots");
        assert_eq!(filled("movies.Xy3kQ9ab.mp4.part", 40, &movie).total, None);
        let archive = filled("archive.Xy3kQ9ab.tar.gz.part", 40, &[published("archive.tar.gz", 100)]);
        assert_eq!(archive.lands_as.as_deref(), Some("archive.tar.gz"));
        let bare = filled("README.Xy3kQ9ab.part", 40, &[published("README", 100)]);
        assert_eq!(bare.lands_as.as_deref(), Some("README"));
    }

    #[test]
    fn odd_totals_are_left_out() {
        let counted_in_percent = filled("movie.mp4.part", 4096, &[published("movie.mp4", 100)]);
        assert_eq!((counted_in_percent.total, counted_in_percent.lands_as.as_deref()), (None, Some("movie.mp4")));
        let unknown = Published { name: "movie.mp4".into(), total: None };
        assert_eq!(filled("movie.mp4.part", 40, &[unknown]).total, None);
        let safari = Partial { name: "song.mp3.download".into(), size: 5, total: Some(50), lands_as: None };
        let mut listing = Listing { partials: vec![safari], ..Listing::default() };
        fill(&mut listing, &[published("song.mp3.download", 80)]);
        assert_eq!(listing.partials[0].total, Some(50), "the bundle's own total stays");
    }
}
