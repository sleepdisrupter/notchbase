# poll

The small pieces the background threads share, so each watcher doesn't invent its own.

- `worker.rs` is a named thread and the shift it works. Dropping the `Worker` stops the thread, and it naps in short steps so a stop is noticed quickly.
- `latest.rs` hands out the newest reading once, and a reading equal to the last one isn't news.
- `outbox.rs` keeps items pushed from any thread in order until they're all taken at once.
- `mailbox.rs` is an outbox that wakes the app each time something is posted.

Most of `src/system` is built from these. See [wiki/System-Tools.md](../../wiki/System-Tools.md#polling-srcpoll).

Run the tests with `cargo test poll::`.
