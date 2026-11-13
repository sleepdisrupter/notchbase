//! Parses a session's icon path into a file and an icon index, expanding variables like %SystemRoot%.

pub fn parse_icon_location(location: &str) -> Option<(String, i32)> {
    let location = location.trim().trim_start_matches('@');
    let (file, index) = match location.rsplit_once(',') {
        Some((file, index)) => (file, index.trim().parse().ok()?),
        None => (location, 0),
    };
    let file = expand_env(file.trim().trim_matches('"'));
    (!file.is_empty()).then_some((file, index))
}

fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match env_value(name) {
            Some(value) => out.push_str(&value),
            None => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn env_value(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_locations_parse_and_expand() {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "%SystemRoot%".into());
        assert_eq!(
            parse_icon_location("@%SystemRoot%\\System32\\AudioSrv.Dll,-203"),
            Some((format!("{root}\\System32\\AudioSrv.Dll"), -203))
        );
        assert_eq!(parse_icon_location("C:\\a.exe"), Some(("C:\\a.exe".to_owned(), 0)));
        assert_eq!(parse_icon_location("\"C:\\a b.exe\",3"), Some(("C:\\a b.exe".to_owned(), 3)));
        assert_eq!(parse_icon_location("C:\\a.exe,x"), None);
        assert_eq!(parse_icon_location(""), None);
    }

    #[test]
    fn env_expansion_keeps_unknowns() {
        assert_eq!(expand_env("%NOTCHBASE_SURELY_UNSET%\\x"), "%NOTCHBASE_SURELY_UNSET%\\x");
        assert_eq!(expand_env("50% off"), "50% off");
        assert_eq!(expand_env("plain"), "plain");
    }
}
