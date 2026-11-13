# assistant

Claude Code and Codex in the notch, through their hooks. The assistant runs `notchbase hook claude` or `notchbase hook codex`, which hands the event to the running app's local server and prints the answer back.

- `hook/` is that hook client. It always exits cleanly, so a problem here never blocks the assistant.
- `server.rs` is the local server on 127.0.0.1, and `endpoint.rs` writes `assistant.json` so hooks can find it.
- `wire.rs` is the one-line JSON protocol between the two, and `token.rs` the secret hooks must show.
- `board.rs` keeps the open prompts and runs, and `prompt.rs` and `runs.rs` describe them.
- `install/` adds our hooks to the assistant's own config and takes them out again, touching nothing else.
- `timing.rs` has how long each kind of prompt waits.

The app's side, turning prompts into questions in the notch, is in `src/app/assistant`. See [wiki/Assistant.md](../../wiki/Assistant.md).

Run the tests with `cargo test assistant`. The tests that run the real binary as a hook are ignored, and need a `cargo build` first.
