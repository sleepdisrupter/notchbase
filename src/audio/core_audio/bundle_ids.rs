//! Bundle ids: helpers fold into the app that owns them, and a tap follows only the bundle ids its app owns.

const HELPER_SEGMENT: &str = "helper";

pub fn parent_bundle_id(id: &str) -> &str {
    let mut offset = 0;
    for (i, part) in id.split('.').enumerate() {
        if i >= 2 && part.eq_ignore_ascii_case(HELPER_SEGMENT) {
            return &id[..offset - 1];
        }
        offset += part.len() + 1;
    }
    id
}

pub fn owned_bundle_ids<'a>(key: &str, ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut owned: Vec<String> = ids.into_iter().filter(|id| is_owned_by(id, key)).map(str::to_owned).collect();
    owned.sort();
    owned.dedup();
    owned
}

fn is_owned_by(id: &str, key: &str) -> bool {
    id.eq_ignore_ascii_case(key)
        || (id.len() > key.len() && id[..key.len()].eq_ignore_ascii_case(key) && id.as_bytes()[key.len()] == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_group_under_their_app() {
        assert_eq!(parent_bundle_id("com.google.Chrome.helper"), "com.google.Chrome");
        assert_eq!(parent_bundle_id("com.google.Chrome.helper.Renderer"), "com.google.Chrome");
        assert_eq!(parent_bundle_id("com.hnc.Discord.Helper"), "com.hnc.Discord");
        assert_eq!(parent_bundle_id("com.spotify.client"), "com.spotify.client");
        assert_eq!(parent_bundle_id("com.apple.helperd"), "com.apple.helperd");
        assert_eq!(parent_bundle_id("helper.helper"), "helper.helper");
        assert_eq!(parent_bundle_id(""), "");
    }

    #[test]
    fn taps_follow_only_owned_bundles() {
        let ids = ["com.google.Chrome.helper", "com.google.Chrome", "com.apple.WebKit.GPU", "com.google.ChromeX"];
        assert_eq!(owned_bundle_ids("com.google.Chrome", ids), vec!["com.google.Chrome", "com.google.Chrome.helper"]);
        assert!(owned_bundle_ids("com.apple.Safari", ["com.apple.WebKit.GPU"]).is_empty());
    }
}
