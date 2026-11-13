//! Turns consent store keys into app names, leaving notchbase itself out.

const PACKAGE_PUBLISHER: char = '_';
const PATH_SEPARATOR: char = '#';
const EXECUTABLE: &str = ".exe";

pub(super) fn apps(keys: impl IntoIterator<Item = String>, me: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in keys.into_iter().map(|key| app_name(&key)) {
        if !name.eq_ignore_ascii_case(me) && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn app_name(key: &str) -> String {
    if key.contains(PATH_SEPARATOR) {
        let file = key.rsplit(PATH_SEPARATOR).next().unwrap_or(key);
        return without_extension(file).to_owned();
    }
    let family = key.split(PACKAGE_PUBLISHER).next().unwrap_or(key);
    spaced(family.rsplit('.').next().unwrap_or(family))
}

fn without_extension(file: &str) -> &str {
    let cut = file.len().saturating_sub(EXECUTABLE.len());
    match file.get(cut..) {
        Some(end) if end.eq_ignore_ascii_case(EXECUTABLE) => &file[..cut],
        _ => file,
    }
}

fn spaced(name: &str) -> String {
    let mut spaced = String::with_capacity(name.len() + 4);
    let mut after_lowercase = false;
    for letter in name.chars() {
        if letter.is_uppercase() && after_lowercase {
            spaced.push(' ');
        }
        after_lowercase = letter.is_lowercase();
        spaced.push(letter);
    }
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_app_named_by_executable() {
        assert_eq!(app_name("C:#Program Files#Zoom#bin#Zoom.exe"), "Zoom");
        assert_eq!(app_name("C:#Users#me#AppData#Local#Discord#app-1.0#Discord.EXE"), "Discord");
        assert_eq!(app_name("D:#tools#recorder"), "recorder");
    }

    #[test]
    fn apps_listed_once_without_us() {
        let keys = ["C:#Apps#Zoom.exe", "C:#Apps#notchbase.exe", "D:#Other#Zoom.exe", "Microsoft.WindowsCamera_8w"];
        assert_eq!(apps(keys.map(String::from), "Notchbase"), ["Zoom", "Windows Camera"]);
    }

    #[test]
    fn store_app_named_by_package() {
        assert_eq!(app_name("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "Windows Camera");
        assert_eq!(app_name("MSTeams_8wekyb3d8bbwe"), "MSTeams");
        assert_eq!(app_name("Slack"), "Slack");
    }
}
