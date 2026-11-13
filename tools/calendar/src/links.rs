//! The calendar links the user added and what each one last loaded.

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::fetch::Tag;
use crate::ics::Feed;
use crate::meeting::Meeting;

#[derive(Clone, Debug, PartialEq)]
pub struct Linked {
    pub link: String,
    pub feed: Option<Feed>,
    pub problem: Option<String>,
    pub fresh: bool,
    pub tag: Tag,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Links {
    pub entries: Vec<Linked>,
}

impl Links {
    pub fn set(&mut self, links: &[String]) -> bool {
        if self.entries.iter().map(|entry| &entry.link).eq(links.iter()) {
            return false;
        }
        let mut old = std::mem::take(&mut self.entries);
        self.entries = links
            .iter()
            .map(|link| match old.iter().position(|entry| entry.link == *link) {
                Some(at) => old.swap_remove(at),
                None => Linked { link: link.clone(), feed: None, problem: None, fresh: false, tag: Tag::default() },
            })
            .collect();
        true
    }

    pub fn loaded(&mut self, link: &str, result: Result<Feed, String>, cached: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.link == link) else { return false };
        if cached {
            let Ok(feed) = result else { return false };
            if entry.fresh || entry.feed.is_some() {
                return false;
            }
            entry.feed = Some(feed);
            return true;
        }
        match result {
            Ok(feed) => {
                entry.feed = Some(feed);
                entry.problem = None;
            }
            Err(problem) => entry.problem = Some(problem),
        }
        entry.fresh = true;
        true
    }

    pub fn meetings(&self, from: u64, to: u64, zone: &TimeZone) -> Vec<Vec<Meeting>> {
        let (Ok(from), Ok(to)) = (Timestamp::from_millisecond(from as i64), Timestamp::from_millisecond(to as i64))
        else {
            return Vec::new();
        };
        self.entries.iter().filter_map(|entry| entry.feed.as_ref()).map(|feed| feed.meetings(from, to, zone)).collect()
    }

    pub fn problems(&self) -> Vec<String> {
        self.entries.iter().filter_map(|entry| entry.problem.clone()).collect()
    }

    pub fn fetched(&mut self, link: &str, tag: Tag) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.link == link) {
            entry.tag = tag;
        }
    }

    pub fn unchanged(&mut self, link: &str) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.link == link) else { return false };
        let cleared = entry.problem.take().is_some();
        entry.fresh = true;
        cleared
    }

    pub fn waiting(&self) -> bool {
        self.entries.iter().any(|entry| entry.feed.is_none() && entry.problem.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(name: &str) -> Feed {
        Feed::parse(&format!("BEGIN:VCALENDAR\nX-WR-CALNAME:{name}\nEND:VCALENDAR")).unwrap()
    }

    fn named(links: &Links) -> Option<String> {
        links.entries[0].feed.as_ref().map(|feed| feed.name.clone())
    }

    #[test]
    fn unchanged_feed_clears_problem() {
        let mut links = Links::default();
        links.set(&["https://a".to_owned()]);
        links.loaded("https://a", Ok(feed("Team")), false);
        links.loaded("https://a", Err("Couldn't reach it".into()), false);
        assert!(links.unchanged("https://a"));
        assert_eq!((named(&links).as_deref(), links.problems().len()), (Some("Team"), 0));
        assert!(!links.unchanged("https://a"), "nothing left to update");
    }

    #[test]
    fn fresh_download_wins() {
        let mut links = Links::default();
        assert!(links.set(&["a".to_owned()]));
        assert!(!links.set(&["a".to_owned()]));
        assert!(links.waiting());
        assert!(links.loaded("a", Ok(feed("Fresh")), false));
        assert!(!links.loaded("a", Ok(feed("Saved")), true));
        assert_eq!(named(&links).as_deref(), Some("Fresh"));
    }

    #[test]
    fn failed_refresh_keeps_last() {
        let mut links = Links::default();
        links.set(&["a".to_owned()]);
        links.loaded("a", Ok(feed("Saved")), true);
        links.loaded("a", Err("Couldn't reach it".into()), false);
        assert_eq!(named(&links).as_deref(), Some("Saved"));
        assert_eq!(links.problems(), ["Couldn't reach it"]);
        links.loaded("a", Ok(feed("Back")), false);
        assert!(links.problems().is_empty());
    }

    #[test]
    fn kept_link_keeps_its_feed() {
        let mut links = Links::default();
        links.set(&["a".to_owned()]);
        links.loaded("a", Ok(feed("A")), false);
        links.set(&["b".to_owned(), "a".to_owned()]);
        assert_eq!(links.entries[1].feed.as_ref().map(|feed| feed.name.as_str()), Some("A"));
        assert!(links.entries[0].feed.is_none());
    }
}
