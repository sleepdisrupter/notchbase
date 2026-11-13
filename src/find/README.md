# find

The file search behind the Find page. It keeps an index of every file in memory, keeps it current from the system's change notifications, and learns from what you open. `Find` (in `mod.rs`) is the app's handle on it: questions go in, and answers come back with a wake.

It's a few threads working together:

- the writer (`writer/`), the only thread that changes the index or the usage model,
- the scanner pool (`scan/`), which lists folders for the writer,
- the watcher (`watch/`), which turns what the system reports into changes,
- the query thread (`asking.rs`), which answers the newest question,
- the icon thread (`icons.rs`), which draws icons by file type,
- the librarian (`apps/`), which reads the apps into a `Book`,
- and on a Mac the recents thread (`recents/`), which asks Spotlight for the files you used lately.

The rest is what they work on: the index in memory (`model/`), answering a question (`query/`), ranking (`rank/`, with every tuning number in `weights.rs`), what Find learns from use, kept in `usage.bin` (`usage/`, `guess/`), saving and loading the index in `index.bin` (`store/`), and the system calls behind it all (`os/`).

The page is in `src/pages/find` and the app's side in `src/app/find`. [wiki/Find.md](../../wiki/Find.md) has the threads, the start-up and the file formats.

Run the tests with `cargo test find::`. Every test uses its own temporary folders, never your home folder. The million-entry timings are ignored and meant for `--release`.
