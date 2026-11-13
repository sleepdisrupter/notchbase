//! Writes the notes to notes.md as a Markdown list.

use std::path::Path;

pub const FILE: &str = "notes.md";

pub fn markdown(notes: &[String]) -> String {
    let mut text = String::from("# Notes\n");
    for note in notes {
        text.push_str("\n- ");
        text.push_str(&note.replace('\n', "\n  "));
    }
    text.push('\n');
    text
}

pub fn write(folder: &Path, notes: &[String]) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|err| format!("Couldn't create {}: {err}", folder.display()))?;
    let path = folder.join(FILE);
    std::fs::write(&path, markdown(notes)).map_err(|err| format!("Couldn't save {}: {err}", path.display()))
}
