# stats

The numbers on the Stats page: hardware gauges, the apps and storage behind them, and Claude or ChatGPT plan usage. Every source works off the main thread, and `Stats::sync` copies what's new into the snapshot the page draws.

- `sampler.rs` is the `notchbase-stats` thread, and `hardware/` reads the CPU, GPU, memory and disk.
- `apps/` is per-app CPU and memory, and `disk/` measures where the disk's space goes.
- Plan usage is read by `crates/usage`, which the dock shares: Claude's from the desktop app's history file first and the usage endpoint only when that's missing or old, and ChatGPT's from the logs the Codex CLI writes.
- `history/` is a local log of plan usage for the week view. It never leaves the computer.

The page is in `src/pages/stats`. See [wiki/Stats.md](../../wiki/Stats.md).

Run the tests with `cargo test stats::`. The hardware tests read the real machine without checking particular values.
