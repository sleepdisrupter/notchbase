# Stats

The Stats page shows four hardware gauges (CPU, GPU, memory, disk) and, when an assistant is chosen in Settings, a card with the plan usage of **Claude** or **ChatGPT** (through Codex). `src/stats` gathers the numbers without blocking the UI; the page is described in [Pages](Pages.md#stats-pagesstats).

## The `Stats` service (`stats/mod.rs`)

`Stats` owns a hardware sampler and two usage trackers, and keeps a `Snapshot`:

| Field | Meaning |
| --- | --- |
| `cpu`, `gpu` | utilisation, 0..1 |
| `gpu_name` | the GPU's marketing name |
| `mem_used`, `mem_total` | bytes of RAM |
| `disk_free`, `disk_total` | bytes on the system volume |
| `listing` | the gauge the page has unfolded, if any: `Cpu` or `Memory` list apps, `Disk` lists storage |
| `storage` | a `Storage`: the measured parts with their largest items, and whether the scan is still running (see [Storage](#storage-statsdisk)) |
| `apps` | an `AppUsage` for each app worth listing (see [Apps](#apps-statsapps)) |
| `claude`, `chatgpt` | a `PlanUsage` each |
| `claude_refreshing`, `chatgpt_refreshing` | whether a refresh is in flight |
| `history` | a `UsageHistory`: the last seven days of usage for each assistant (see [History](#history-statshistory)) |

The app only samples while the Stats page is visible, once per `stats.refresh_ms` (1 second by default, at least 250 ms; `Stats::sample`). Each background source publishes into shared state and calls the app's wake closure; `Stats::sync` then copies what is new into the snapshot on the main thread. A manual refresh (clicking the usage card) calls `Stats::refresh_usage`. The assistant whose usage is shown is `assistant.provider` (when `assistant.enabled` is on); see [Configuration](Configuration.md).

## Hardware (`stats/sampler.rs`, `stats/hardware`)

The `notchbase-stats` thread owns a `Probe`. It publishes the static facts (GPU name, disk) immediately, waits up to a second for the first request, and then takes a sample for each request, never more often than every 400 ms, so bursts of requests collapse into one sample.

- **CPU** and **memory** come from the `sysinfo` crate (global CPU usage, used and total RAM). CPU usage needs two readings, so the first sample reports nothing.
- **Disk** is the system volume's free and total space, re-read at most every 10 seconds.
  - Windows: `GetDiskFreeSpaceExW` on `%SystemDrive%` (falling back to `C:`).
  - macOS: the boot volume's capacity keys through `CFURLCopyResourcePropertyForKey`, including the "available for important usage" figure that matches what Finder shows.
- **GPU**:
  - Windows: a PDH query on the counter `\GPU Engine(*engtype_3D)\Utilization Percentage`, summing every 3D engine instance. The name is the first non-software DXGI adapter's description.
  - macOS: the `IOAccelerator` service's `PerformanceStatistics` dictionary, key `Device Utilization %`, read through IOKit. The name comes from the accelerator's `model` property, with "GPU" appended when it is missing ("Apple M4" becomes "Apple M4 GPU").

## Plan usage

Both trackers produce a `PlanUsage`:

```rust
enum PlanUsage {
    Loading,
    Ready { five_hour: Option<UsageWindow>, weekly: Option<UsageWindow>, fetched_at: SystemTime },
    SignedOut,
    Expired,
    Error(String),
}
```

A `UsageWindow` is the fraction used, when it resets, the window length in minutes, and whether the value is an estimate. The card shows the two windows as **5-hour** and **Weekly** bars.

Each tracker refreshes on its own short-lived thread (`notchbase-claude`, `notchbase-codex`). While the page is visible a refresh can start every 3 seconds; a manual refresh is debounced to 2 seconds. Only one refresh per tracker runs at a time. Results carry a version number so the snapshot only changes when the usage actually changed.

### Claude (`crates/usage/src/claude`)

Claude usage is read from two sources, preferring the one that costs nothing:

1. **The Claude desktop app's history file** (`claude/desktop.rs`). The app keeps a `plan-usage-history.json` file with recent usage samples. notchbase looks for it in:
   - Windows: `%APPDATA%\Claude`, and, for the Microsoft Store (MSIX) build, `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude`, because packaged apps keep their AppData inside the package;
   - macOS: `~/Library/Application Support/Claude` and `~/Library/Application Support/Claude-3p`;
   - elsewhere: `$XDG_CONFIG_HOME/Claude` or `~/.config/Claude`.

   The newest file wins. It is parsed defensively (schema version 2, size-limited to 8 MB, retried briefly if it is being written) and cached by path, size and modification time. The file's samples give the 5-hour and weekly percentages; their reset times are inferred by finding where the current window started in the series (a drop after a reset, or the first use after being idle) and adding the window's length, so windows read from the history are marked as estimates. If the latest sample is at most **20 minutes** old, it is used as is.

2. **The Anthropic usage endpoint** (`claude/api.rs`). If the history is missing or stale, notchbase asks `https://api.anthropic.com/api/oauth/usage` with the OAuth access token of your Claude Code login, at most **every 5 minutes** (every 30 seconds for a manual refresh). The token is read from (`claude/credentials`):
   - on macOS, the login Keychain item "Claude Code-credentials" (and the variants Claude Code uses for a custom config directory, suffixed with part of the SHA-256 of the directory), through `/usr/bin/security`;
   - the `.credentials.json` file in `CLAUDE_CONFIG_DIR` or `~/.claude`.

   Tokens are never logged (their debug form is redacted), requests are HTTPS-only with no redirects and a 10 second timeout, and a Keychain token is kept in memory between refreshes so the Keychain is not asked every time. The response's `five_hour` and `seven_day` windows (and the newer `limits` list) are parsed into usage windows.

How failures are handled (`claude/state.rs`, `claude/refresh.rs`):

- `401` means the login expired: the card shows "Expired", after first retrying once with freshly read credentials if the token came from the cache.
- `429` honours `Retry-After` (up to an hour) before asking the endpoint again.
- If macOS denies Keychain access, notchbase stops asking until you refresh manually, and the card says "Allow Keychain access to read Claude usage".
- If the endpoint fails but a history file exists, the (older) history is shown instead.
- Errors never replace good numbers younger than 30 minutes: the card keeps showing the last good usage.

### ChatGPT through Codex (`crates/usage/src/codex`)

ChatGPT plan usage is read from the files the Codex CLI writes, with no network access at all:

1. The Codex home is `CODEX_HOME` or `~/.codex` (`codex/home.rs`).
2. `codex/rollouts.rs` lists session logs: `sessions/YYYY/MM/DD/rollout-*.jsonl` for the 7 most recent days (the 64 newest), plus the 16 newest in `archived_sessions`, sorted by modification time.
3. `codex/tail.rs` reads each file **backwards** from the end in 64 KB chunks (at most 2 MB per file, 8 MB and 8 files per scan), looking for the last `event_msg` of type `token_count` that carries `rate_limits` for the main `codex` limit.
4. `codex/snapshot.rs` parses the primary and secondary windows (percent used, window length, reset time or seconds until reset), and `codex/slots.rs` places them into the 5-hour and weekly slots by their length. If a window's reset time has already passed, it is shown as 0% and marked as an estimate.

Results are cached per file (path, size, modification time) so an unchanged log is not re-read. If there is no Codex home or no usage in any log, the card says "Use Codex on this computer to see ChatGPT usage".

## History (`stats/history`)

notchbase keeps a small local log of plan usage so the Stats page can show when you burn through your limits.

**What is logged.** Every time a tracker hands `Stats::sync` a new `PlanUsage::Ready`, one line is appended to the log: `{"at_ms":…,"provider":"claude","five_hour":0.42,"weekly":0.17}`. `at_ms` is the usage's `fetched_at` (when the numbers were true, not when they were read), `provider` is `claude` or `codex`, and a window the plan doesn't have is left out. Usage with neither window is not logged. A sample is only kept if it is newer than the last one for that assistant, and identical numbers are logged at most every 10 minutes, so re-reading the same history file or rollout never adds lines. Nothing is logged while a [preview](Previews.md) runs. Usage is only fetched while the Stats page is visible, so the log only covers those times.

**Where.** `usage-history.jsonl` in the data directory (`%APPDATA%\notchbase` on Windows, `~/Library/Application Support/notchbase` on macOS, or `NOTCHBASE_DATA_DIR`). It never leaves your computer.

**Retention.** The `notchbase-history` thread loads the file once at start (at most 4 MB of it) and then owns it, appending samples as they arrive over a channel. On load, unreadable lines are skipped and samples older than **14 days** are dropped; if anything was dropped the file is rewritten (through a temporary file and a rename). When the file grows past **2000 lines** it is compacted the same way down to the newest 1500 samples. A missing or broken file is never fatal: the page simply has less history.

**The week.** `history/week.rs` turns the samples into a `UsageHistory` with one `Week` per assistant: seven `Day { peak, hit_limit, weekday }`, six days ago through today in local time (the UTC offset comes from the platform's local clock, `platform::utc_offset_secs`). `peak` is the day's highest 5-hour usage (the weekly usage for a plan without a 5-hour window), `hit_limit` is whether either window reached 100% that day, and `weekday` is the day's initial. The weeks are recomputed when a sample arrives, when the file finishes loading, and after local midnight, and `Snapshot::history` only changes when they do.

## Apps (`stats/apps`)

While the Stats page lists apps (`Stats::list_apps`, from `Action::ListApps`), every sample request tells the `notchbase-stats` thread so, and the `Probe` reads the apps at most every 1.5 seconds. When the list is folded the reader is dropped, so nothing is measured for a list nobody sees. This is macOS only (`LISTS_APPS`); elsewhere the gauges are not clickable and the reader returns nothing.

- **Reading** (`apps/macos.rs`): `proc_listallpids`, then `proc_pid_rusage` (`RUSAGE_INFO_V4`) for each process: CPU time (user plus system, in Mach ticks) and the physical footprint, the memory figure Activity Monitor shows. Processes owned by other users, such as WindowServer and `kernel_task`, can't be read without privileges and are skipped.
- **Grouping**: a process belongs to the app macOS holds responsible for it (`responsible_pid`, shared with the [mixer](Sound-Mixer.md)), keyed by that app's bundle identifier with helper suffixes removed, so a browser's helpers and whatever runs inside a terminal count toward the app. Processes whose owner is not an app (no `NSRunningApplication`, or one that may not activate) are left out.
- **CPU** is the group's CPU time since the previous read, divided by the time between reads and the number of cores: a share of the whole machine, like the gauge, not a share of one core. The first read after the list opens waits 250 ms and reads again so the first list already has shares.
- **What is published**: the five heaviest apps by CPU and the five heaviest by memory (up to ten), each with a name and icon loaded once per app. The page picks and orders the five for the chosen resource (`stats::top`), and shows the gauge's value minus those five as "Everything else".
- **Quitting** (`Stats::quit_app`, on the main thread): only an app that is in the snapshot and marked `quittable` is touched. Every running instance with that bundle identifier gets `terminate`, or `forceTerminate` when forced. An app can be quit unless it is notchbase itself, Finder, or an Apple agent without a Dock icon.

## Storage (`stats/disk`)

Opening the storage view (`Stats::list_apps(Some(Disk))`) starts a scan on the `notchbase-disk` thread, unless one is running or the last one is less than ten minutes old; while the view stays open it is measured again when that goes stale. Four workers at utility priority take roots from a queue, and after each root the `Storage` so far is published, so the view fills in as the scan goes (about 12 seconds for 190 GB on an M-series MacBook). App icons are loaded once, after the last root.

**Parts.** A `Storage` is a list of `PartUsage { part, size, items }`, largest first; a part under 8 MB is left out. `plan.rs` says which `Part` each root belongs to and how to list it:

| Root | Part | Items |
| --- | --- | --- |
| `/Applications`, `~/Applications` | Apps | each app |
| `~/Documents`, `~/Desktop`, `~/Downloads` | their own parts | large files, then top folders |
| `~/Pictures`, `~/Movies` | Photos and video | large files, then top folders |
| `~/Music` | Music | large files, then top folders |
| `~/Library/Application Support`, `Containers`, `Group Containers`, `Caches` ("Chrome cache"), the rest of `~/Library` | App data | each child |
| `~/Library/Developer`, `/opt`, `~/.cargo`, `~/.rustup`, `~/.npm` and other toolchain folders | Developer | each one whole |
| `~/Parallels`, `~/Virtual Machines`, `~/UTM` | Virtual machines | each machine |
| `/Library` | System and other | each child |
| everything else in the home folder, the Trash, `/Users/Shared` | Other files | large files, then top folders |

**What is inside overrides where it is** (`scan.rs`). Wherever the walk finds a `node_modules` or `DerivedData` folder, or a `target` folder beside a `Cargo.toml`, it goes to **Developer** as "project folder", not to the part it sits in. The view only says what things are; it never suggests what to delete. A virtual machine (`.pvm`, `.utm`, `.vmwarevm`, or a `.vdi`, `.vmdk`, `.qcow2`, `.hdd`, `.vhdx` disk) goes to **Virtual machines**. A package (`.app`, `.photoslibrary`, `.bundle` and the like) is one item, never opened up. In a files part, any file of 50 MB or more is an item wherever it is nested, and each top-level folder is an item for what it holds beyond those files, so the items add up to the part. Reverse-domain names become readable (`com.google.Chrome` is "Chrome").

The walk does not follow symlinks or leave its volume, skips iCloud folders that are not downloaded (the `SF_DATALESS` flag, so nothing is fetched), and counts allocated blocks, so sizes are what things take on disk. A folder macOS will not let notchbase read counts as nothing and ends up in "System and other", which the page computes as the volume's used space minus everything measured. Without Full Disk Access, macOS asks once each for Desktop, Documents and Downloads the first time they are scanned.

## Tests

`stats/history` tests the sample format and corrupt lines, de-duplication and the 10 minute rule, pruning and thinning, loading, appending and compacting in temporary folders, the writer thread (including a read-only history that never writes), and day bucketing in UTC and New York time. `crates/usage` (shared with the dock) has tests for Claude's history file format and reset detection, the MSIX package lookup, credentials parsing and Keychain output decoding, the API response parser, RFC 3339 parsing, and the refresh and hold state machine, and for ChatGPT's rollout discovery, backward tailing across chunk boundaries, snapshot parsing, slot placement and resets, using fixture files in temporary folders. `stats/disk` measures small trees in temporary folders (large files listed flat with folders keeping the rest, build folders and machines leaving the part they sit in, readable names) and checks which part each home folder goes to. `stats/apps` tests the ranking, the leaders that get published, the rule for what may be quit, and reads the real machine without asserting particular values. `stats/hardware` has smoke tests that read the real machine without asserting particular values.
