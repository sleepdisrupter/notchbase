# whats_new

The notes each release carries, built into the app so they work offline and always match the copy you're running. `notes.rs` has the notes, newest first, and a line that's only true on one system is only built there.

The page is in `src/pages/whats_new` and the "Updated" notice in `src/notch/notice/whats_new.rs`. [What's New in wiki/Updates.md](../../wiki/Updates.md#whats-new) explains how to write the notes for a release.

Run `cargo test whats_new` after editing the notes. The tests check the versions, the wording and the pictures.
