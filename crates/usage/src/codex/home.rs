//! The Codex home: CODEX_HOME, or .codex in the user's folder.

use std::ffi::OsString;
use std::path::PathBuf;

pub(super) fn locate(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| var(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(home) = var("CODEX_HOME") {
        return Some(home);
    }
    let user = if cfg!(windows) { var("USERPROFILE").or_else(|| var("HOME")) } else { var("HOME") };
    user.map(|dir| dir.join(".codex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(vars: &[(&str, &str)]) -> Option<PathBuf> {
        locate(|name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(value)))
    }

    #[test]
    fn codex_home_wins_when_set() {
        assert_eq!(
            with(&[("CODEX_HOME", "/srv/codex"), ("HOME", "/u"), ("USERPROFILE", "/u")]),
            Some("/srv/codex".into())
        );
        assert_eq!(
            with(&[("CODEX_HOME", ""), ("HOME", "/u"), ("USERPROFILE", "/u")]),
            Some(PathBuf::from("/u").join(".codex"))
        );
    }

    #[test]
    fn falls_back_to_the_user_directory() {
        assert_eq!(with(&[]), None);
        let home_only = with(&[("HOME", "/h")]);
        assert_eq!(home_only, Some(PathBuf::from("/h").join(".codex")));
        let both = with(&[("HOME", "/h"), ("USERPROFILE", "/p")]);
        let expected = if cfg!(windows) { "/p" } else { "/h" };
        assert_eq!(both, Some(PathBuf::from(expected).join(".codex")));
        let profile_only = with(&[("USERPROFILE", "/p")]);
        assert_eq!(profile_only, cfg!(windows).then(|| PathBuf::from("/p").join(".codex")));
    }
}
