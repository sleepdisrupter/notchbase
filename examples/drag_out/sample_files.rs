//! Two small temp files to drag, one of them inside a folder.

use std::path::PathBuf;

const CONTENTS: &str = "Dragged out of notchbase's drag_out example.\n";

pub fn create() -> Vec<PathBuf> {
    let temp = std::env::temp_dir();
    let files = vec![temp.join("notchbase drag test.txt"), temp.join("notchbase drag test").join("second file.md")];
    for file in &files {
        let folder = file.parent().expect("sample file has a folder");
        std::fs::create_dir_all(folder).expect("create sample folder");
        std::fs::write(file, CONTENTS).expect("write sample file");
    }
    files
}
