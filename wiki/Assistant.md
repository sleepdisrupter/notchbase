# Assistant: Claude Code and Codex in the notch

When Claude Code or Codex (ChatGPT's coding agent) needs you, for example to approve a command, answer a question or look at a finished plan, notchbase can show it in the notch and let you answer from there, without switching to the terminal. While they work, a small live activity shows it, and a short notice says when a turn is done. `src/assistant` implements this with the assistants' **hooks**: small commands the assistant runs at points in its lifecycle. notchbase installs itself as those hooks.

Choose the assistant in [Settings › Assistant](Pages.md#assistant-pagessettingsassistantrs) and turn on "Questions in the notch", "Progress in the notch" or both; the same choice selects whose plan usage the [Stats](Stats.md) page shows. They are stored in `config.json` as `assistant.enabled`, `assistant.provider` (`claude` or `chatgpt`), `assistant.questions_in_notch` and `assistant.progress_in_notch` (see [Configuration](Configuration.md)).

## The pieces

```
Claude Code / Codex
   |  runs a hook:  notchbase hook claude   (payload on stdin)
   v
hook client  (the notchbase executable, short-lived)
   |  reads <data dir>/assistant.json  ->  port + token
   |  one JSON line over TCP to 127.0.0.1:<port>
   v
server  (inside the running app)
   |  applies the event to the prompt board
   |  holds the connection open if it needs an answer
   v
app  ->  notch Ask mode  ->  user clicks Allow / Deny / an option
   |
   v
server writes the answer line  ->  hook client prints the hook decision  ->  assistant continues
```

| File | Role |
| --- | --- |
| `assistant/hook/mod.rs` | the hook client entry point (`notchbase hook ...`) |
| `assistant/hook/claude.rs`, `hook/codex.rs` | map each assistant's hook payloads to events |
| `assistant/hook/describe.rs`, `hook/fingerprint.rs` | human summaries of tool calls, and stable keys for them |
| `assistant/hook/client.rs`, `hook/render.rs` | talk to the server, and print the decision the assistant expects |
| `assistant/hook/window` | find the terminal, IDE or app window the assistant runs in |
| `assistant/wire.rs` | the line protocol shared by client and server |
| `assistant/call.rs` | calls from a second notchbase launch to the running one (`show`) |
| `assistant/endpoint.rs`, `assistant/token.rs` | publishing the port and a secret token |
| `assistant/server.rs`, `assistant/board.rs` | the local server and the board of open prompts |
| `assistant/runs.rs` | which turns are running, which are waiting on you, and which just finished |
| `assistant/prompt.rs`, `assistant/timing.rs` | prompt types, and how long things are held and kept |
| `assistant/install` | adding and removing notchbase's hooks in the assistants' config files |
| `app/assistant/mod.rs`, `app/config.rs` | turning board prompts into the notch's `Ask` and replies into answers; connecting hooks when the configuration changes |
| `app/assistant/agents.rs`, `notch/live/agents.rs` | the progress live activity and the "done" notice |
| `app/assistant/view.rs`, `pages/agents` | the Agents page: every live run, what it waits on, and the recent finished turns |

## The hook client

`main` checks for hook mode before anything else starts (see [Architecture](Architecture.md#one-binary-two-modes)). `assistant::run_hook_from_args` returns `None` unless the first argument is `hook`. In hook mode:

- Panics are silenced and caught, and the process **always exits with code 0** and prints either nothing or a valid decision. A problem in notchbase must never break or block the user's assistant.
- The provider comes from the second argument (`claude` or `codex`). The payload is read from stdin as JSON (up to 64 MB).
- `assistant.json` is read from the data directory. **If notchbase is not running**, the file is missing or the connection is refused within 250 ms, the client prints nothing and the assistant falls back to its own prompt.
- The payload becomes a `Plan`: an `Event` (provider, session id, agent id and type if a subagent is involved, working directory, start time, the window it runs in, and a `Signal`) plus a `Render` that knows how to print an answer for this hook.
- The client sends one line `{ "token": ..., "event": ... }`. For signals that do not need an answer it exits immediately. For an `Ask` it waits for the answer line and prints the decision.

### Finding the assistant's window (`hook/window`)

The hook runs as a child of the assistant, which was started by a shell, which was started by a terminal, an IDE or the Claude app. Once notchbase is known to be running, the client walks up its own ancestors, at most 12, and stops at the first one that owns a window you could bring to the front; that process id goes in the event's `window`, or nothing when none is found. The walk itself (`window::nearest`) is the same everywhere; each platform says who a process's parent is and whether it has a window:

| | Parent | Has a window |
| --- | --- | --- |
| Windows (`window/windows.rs`) | one `CreateToolhelp32Snapshot` of every process; the walk stops at the shell (Explorer, the owner of `GetShellWindow`) | `EnumWindows` finds a visible, unowned top-level window without `WS_EX_TOOLWINDOW` whose `GetWindowThreadProcessId` is that process (`platform::windowed_processes`) |
| macOS (`window/macos.rs`) | `proc_pidinfo` with `PROC_PIDT_SHORTBSDINFO`; the walk stops at `launchd` | `NSRunningApplication(processIdentifier:)` has the regular activation policy |

So a run started in Windows Terminal, a classic console (whose window counts as the shell's), VS Code (its main process) or the Claude app points at that window. Like everything in the hook, a failure here just leaves `window` empty.

Codex also supports a legacy `notify` program that receives a JSON argument instead of stdin; `notchbase hook codex '<json>'` handles its `agent-turn-complete` notification as a turn ending.

### Which hooks mean what

**Claude Code** (`hook/claude.rs`):

| Hook event | Signal |
| --- | --- |
| `PermissionRequest` | `Ask` with a permission prompt (offering **Always allow** when the payload carries `permission_suggestions`), or the questions of `AskUserQuestion`, or an attention prompt "Plan ready for review" for `ExitPlanMode` |
| `PreToolUse` | `ToolStarted` with the tool's name, so progress can say what Claude is doing |
| `PostToolUse`, `PostToolUseFailure` | `ToolDone` for that tool call |
| `UserPromptSubmit` | `TurnStarted` |
| `Stop` | `TurnEnded` with the last assistant message |
| `Notification` | `Attention` for permission prompts, elicitation dialogs, "agent needs input" and stale quota auto-resume notices; `Idle` for `idle_prompt`, which Claude sends once it has sat waiting for input |
| `SessionEnd` | `SessionEnded` |

**Codex** (`hook/codex.rs`):

| Hook event | Signal |
| --- | --- |
| `PermissionRequest` | `Ask` with a permission prompt |
| `PreToolUse` for `request_user_input` | `Note` with the question (Codex asks these itself; notchbase shows them but does not answer) |
| `PostToolUse` | `ToolDone` |
| `UserPromptSubmit` | `TurnStarted` |
| `Stop` | `TurnEnded` |
| `SessionEnd` | `SessionEnded` |

### Describing a tool call (`hook/describe.rs`)

A permission prompt needs a title and a one-glance detail. The tool name is kept, with MCP tools shown as `server/tool`. The detail is the first meaningful field of the tool input (`command`, `file_path`, `notebook_path`, `url`, `query`, `pattern`, `path`, `prompt`, `description`); for `apply_patch` it lists the files the patch touches; otherwise it is the compact JSON of the input. Details are clipped to 600 characters, question text to 400, options to 120 (at most 8), and markdown markers are stripped from messages. A message (the last assistant message of a finished turn, or a notification's text) keeps its line breaks: whitespace is collapsed within each line, empty lines are dropped, and the remaining lines are joined with a single newline before the 400 character clip, so the Agents page can show its first line as a summary.

`AskUserQuestion` asks one to four questions. Each becomes a `Question` with its short `header`, its text, its options (a label and a description each, at most 8) and whether it is multi-select. The prompt is **answerable** in the notch when every question has text and options and none were dropped; otherwise, and for Codex's `request_user_input` (which notchbase can't answer), it is shown with the first question's options listed and a button to answer in the assistant.

### Fingerprints (`hook/fingerprint.rs`)

Every tool call gets a 64-bit FNV-1a key computed from the tool name and a canonical (sorted-key) JSON of its most identifying input field. The `PermissionRequest` and the later `PostToolUse` for the same call produce the same key. That is how the board knows a prompt was **answered somewhere else**: if you approve in the terminal, the tool runs, its `PostToolUse` arrives as `ToolDone`, and the matching prompt disappears from the notch and its held hook is released.

### Printing the decision (`hook/render.rs`)

Only permission requests print anything. The output is the assistants' `PermissionRequest` hook format:

- **Allow**: `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`
- **Deny**: behaviour `deny` with the message "The user denied this request from notchbase." (for Claude also `"interrupt": true`, which stops the turn).
- **Always** (Claude permissions with suggestions): behaviour `allow` with `updatedPermissions` set to the `permission_suggestions` Claude sent, the same as choosing "Yes, and don't ask again" in Claude. Without suggestions, or for Codex, it is a plain allow.
- **Answers** (Claude questions): behaviour `allow` with `updatedInput` equal to the original input plus an `answers` map from each question's text to the chosen option's label; the labels of a multi-select question are joined with ", ". Every question must be answered, or nothing is printed.
- **Release** (answer elsewhere, timeout, notchbase quitting): no output, so the assistant shows its own prompt as if notchbase were not there.

## The wire protocol (`wire.rs`)

Newline-delimited JSON over TCP on the loopback interface.

- Request: one line, `Envelope { token, event }`, at most 256 KB. The event's `window` (a process id) is left out when unknown, and an event without it, from an older hook, reads as unknown.
- Call: one line, `{"token":"...","call":{"type":"show"}}`, sent by a second launch of notchbase that found this one already running (see [Architecture](Architecture.md#one-running-copy-per-data-directory)). The server counts it and wakes the app, which shows "notchbase is already running" in the notch. Nothing is answered. The server reads a line as an event when it has `event` and as a call when it has `call` (`call::Inbound`), so hooks are unchanged.
- Answer (only for `Ask`): one line, `{"answer":"allow"}`, `{"answer":"always"}`, `{"answer":"deny"}`, `{"answer":"answers","picks":[[1],[0,2]]}` (the chosen option indexes of each question) or `{"answer":"release"}`, at most 1 KB.

`Signal` is tagged by `type`: `ask`, `note`, `attention`, `tool_started`, `tool_done`, `turn_started`, `turn_ended`, `idle`, `session_ended`. Prompt kinds are tagged by `kind`: `permission`, `questions`, `attention`, `finished`.

## The server (`server.rs`, `endpoint.rs`, `token.rs`)

`Assistant::start` (called when the app starts) creates the server:

1. Generate a **32-byte random token**, hex encoded: from `/dev/urandom` on macOS, from two `CoCreateGuid` calls on Windows.
2. Bind a TCP listener on `127.0.0.1` with an OS-assigned port.
3. **Publish** `{ "port": ..., "token": ... }` to `assistant.json` in the data directory: written to a temporary file (mode `0600` on Unix) and renamed into place.
4. Start `assistant-accept`, which spawns one `assistant-hook` thread per connection (at most 64 at a time), and `assistant-sweep`, which expires prompts and stale runs once a second and every 5 seconds ends runs whose window process has exited.

Each connection gets 3 second read and write timeouts for its request line. The token is compared in constant time; a wrong token is dropped silently. Only processes that can read the user's data directory can talk to the server.

On quit, `Assistant` is dropped: every held prompt is released (so waiting hooks print nothing and the assistants fall back to their own prompts), the accept loop is unblocked, and `assistant.json` is removed if it still holds this instance's token.

If the port cannot be opened, the app logs that questions in the notch are unavailable and carries on.

## The board (`board.rs`, `prompt.rs`, `timing.rs`)

The board is the list of prompts the notch can show. A `Prompt` has an id, the provider, the project (the working directory's name), an optional agent type, a `PromptKind` and a timestamp:

- `Permission { tool, detail, always }`
- `Questions { questions, answerable }`
- `Attention { message }`
- `Finished { message }`

How each signal changes the board:

| Signal | Effect |
| --- | --- |
| `Ask` | add a prompt and **hold** the connection until it is answered, released or times out |
| `Note` | add a prompt that expires after 30 minutes |
| `Attention` | add an attention prompt that expires after 15 minutes, replacing older attention and finished prompts from the same session; ignored if that session already has a held prompt |
| `ToolDone` | remove and release the prompt with the same fingerprint from the same agent |
| `TurnStarted` | remove that agent's older prompts (the user moved on) |
| `TurnEnded` | remove that agent's older prompts and add a `Finished` prompt that expires after 10 minutes |
| `Idle` | nothing on the board; it only ends the session's run (see [Progress](#progress)) |
| `SessionEnded` | remove every older prompt of that session |

How long a held prompt waits before it is released (`timing.rs`):

| Provider | Hold |
| --- | --- |
| Claude, main session | 4 hours |
| Claude, inside a subagent | 25 seconds |
| Codex | 5 minutes |

The hooks are installed with a timeout of the hold plus one minute, and the client waits a little longer than the longest hold, so notchbase always answers (or releases) before the assistant gives up on the hook.

Turning a provider off retires all its prompts and ignores its events until it is turned back on. Every change bumps the board's version and wakes the app.

## From the board to the notch

In `app/assistant/mod.rs`, on each wake-up the app compares the board version with the last one it saw. If it changed, or the assistant settings that decide the hooks just changed (so turning "Questions in the notch" off takes an open question down at once), it picks the first prompt that is not `Finished` (finished turns are kept on the board but never pop the notch) and builds a `bridge::Ask`:

- **context**: the project, plus " · agent type" for subagents;
- **title**: for permissions, a question matching the tool: "Run this command?" for shell tools (`Bash`, `shell`, `exec_command`, `local_shell`), "Change this file?" for edit tools (`Edit`, `MultiEdit`, `Write`, `NotebookEdit`, `apply_patch`), "Read this file?", "Open this page?", "Search the web?", or "Allow <tool>?" with MCP names made readable ("Allow github create issue?"); for questions and attention, the text itself;
- **detail**: the command or path for permissions, or the options joined with " · " for questions that must be answered elsewhere;
- **choices**: `Permission { always }`, `Questions(questions)` for answerable questions, or `Elsewhere`;
- **waiting**: how many more prompts are queued.

Every notch receives it through `show_ask` and switches to Ask mode (see [Notch](Notch.md#ask-prompts)). The buttons return `Action::Answer(id, reply)` with `Allow`, `AllowAlways`, `Deny`, `Answers(picks)` or `InApp` ("Answer in Claude" / "Answer in ChatGPT"). `InApp` releases the prompt so the assistant asks it in its own window, and when the prompt's run knows its window (`Assistant::window_of`, which looks up the live run for the prompt's provider and session on the board), the app follows it with `Action::FocusWindow` so that terminal, IDE or app comes to the front, the same way the Open pill on the Agents page does. The server maps the reply to an answer for that prompt's kind: `AllowAlways` on a permission without suggestions is a plain allow, answers must pick exactly one option of each single-choice question and at least one of each multi-choice one (`prompt::fits`), and anything else becomes a release.

With "Questions in the notch" off, prompts still reach the board (so a waiting run can say so) but never open the notch on their own. Clicking a waiting run on the [Agents page](Pages.md#agents-pagesagents) pins its prompt (`Action::ShowPrompt`): the app shows that prompt in Ask mode ahead of the queue, whatever the setting, until it is answered or leaves the board.

## Progress

A **run** is one turn of one session: it starts when you send a prompt (`TurnStarted`) and ends when the assistant stops (`TurnEnded`). `assistant/runs.rs` keeps them on the board beside the prompts:

| Signal | Effect on the run |
| --- | --- |
| `TurnStarted` | starts a run for the session, or restarts it from the new turn |
| `ToolStarted` | the session is doing something: the tool's name becomes a verb (`runs::verb`: Running, Editing, Reading, Searching, Browsing, Delegating, Planning, Using tools, or Working) |
| `ToolDone` | the session is thinking again: starts a run if none is known (notchbase started mid-turn) and clears "waiting" and the verb |
| `Ask`, `Note`, `Attention` | the session is waiting on you |
| an answer from the notch | clears "waiting" once the session has no other open prompt (except "Answer in Claude", which leaves it to the assistant) |
| **Deny** on a Claude permission | ends the run quietly, like `Idle`: Claude is told to interrupt the turn and sends no `Stop`. Codex keeps its run |
| a prompt leaving the board any other way | when a prompt is released because it timed out, its hook hung up, the turn or session moved on, it expired or a `ToolDone` settled it, and the session has no other prompt left that isn't a finished turn, "waiting" clears |
| `TurnEnded` | ends the run and records it as **finished**, with how long it took, when it ended and the assistant's last message (the last eight are kept, and each for at most three hours) |
| `Idle`, `SessionEnded` | ends the run quietly. Claude skips its `Stop` hook when you interrupt a turn, and `idle_prompt` then ends the run instead |
| its window's process exiting | ends the run quietly, like `SessionEnded`, and takes the session's prompts with it (see below) |

A run also keeps the process id of the window it runs in (`Run.window`): the latest one any of its hooks reported, so a later hook that couldn't find one never erases it.

A session whose terminal is closed mid-turn never sends `SessionEnded`. So every 5 seconds the server's sweep thread asks `Board::expire_gone` to end each live run whose window process no longer exists, checked with `platform::process_alive` (Windows: `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION` and `GetExitCodeProcess` still `STILL_ACTIVE`, where access denied counts as alive; macOS: `kill(pid, 0)` succeeds or fails with `EPERM`). The run ends quietly with no finished turn, its session counts as ended from that moment so a late hook can't bring it back, and its prompts are released. `Runs` takes the check as a closure, so it never calls the platform itself. A run without a known window still waits out the 15 minutes.

Hook processes carry the time they started (`started_ms`), and a session remembers when its last run ended, so a background hook that lands late (a `PostToolUse` racing the `Stop`) never brings a finished turn back. A run nobody hears from for 15 minutes is dropped (a long Bash call can be silent for up to 10), a finished turn leaves the recent list three hours after it ended, and turning the provider off drops its runs.

The app (`app/assistant/agents.rs`) turns runs into the notch:

- **Working**: a run that has lasted **10 seconds** shows as an `Activity::Agent`, so quick answers never flicker the notch. The notch draws the assistant's own mark (the Claude spark in Claude's orange, or the ChatGPT knot) and changes it with the mood: **thinking** turns it slowly beside the time since the turn began, **doing** spins it faster beside a verb such as "Editing", and several sessions add a count. See [Notch](Notch.md#the-live-activity).
- **Needs you**: while a run waits on you the mark turns yellow and stops, and the right side says "Needs you". With "Questions in the notch" on, the prompt itself opens the notch anyway.
- **Alongside other things**: the assistant is one of up to three activities; with music playing or a recording going it becomes a liquid ear beside them. See [Notch](Notch.md#several-activities-notchlivesidesrs).
- **Clicking it**: clicking the live activity, or the assistant's ear, opens the [Agents page](Pages.md#agents-pagesagents). While a run waits, the Agents tab carries a yellow dot, and each run whose window is known has an **Open** pill that brings that terminal, IDE or app to the front (`Action::FocusWindow`, see [Pages](Pages.md#agents-pagesagents)).
- **Done**: when a run that lasted at least 10 seconds finishes, a small notice with a green checkmark says "<project> done" for about three seconds.

A video download takes the notch before the assistant's live activity, the assistant's before a tool's (a recording, a timer), and all of them come before the updater's. Updates also wait until no run is going, because installing restarts notchbase and would drop the hooks' connections.

## Installing the hooks (`assistant/install`)

When an assistant is chosen and questions or progress in the notch are on, the app installs notchbase's hooks for it (at start-up, and whenever the settings change, whether from the Settings page or by editing `config.json` while notchbase runs); when you switch assistants or turn both off, it removes them from the one you left. With only progress on (`Hooks::Progress`), the answering hooks (`PermissionRequest`, and Codex's `PreToolUse` for questions) are left out, so notchbase never holds up the assistant. The board only accepts events from the chosen provider. Hooks are never touched while a [preview](Previews.md) is running.

| | Claude Code | Codex |
| --- | --- | --- |
| Config file | `settings.json` in `CLAUDE_CONFIG_DIR` or `~/.claude` | `hooks.json` in `CODEX_HOME` or `~/.codex` |
| Command | the executable path, with args `["hook", "claude"]` | a shell command: the quoted executable path followed by `hook codex` |
| Blocking hook | `PermissionRequest` for every tool, with a timeout of the hold plus a minute | `PermissionRequest` for every tool, timeout of the hold plus a minute, status message "Waiting for notchbase" |
| Other hooks | `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `Stop` and `Notification` run asynchronously; `SessionEnd` runs quickly in the foreground | `PreToolUse` for `request_user_input` (10 s timeout); `PostToolUse`, `UserPromptSubmit` and `Stop` asynchronously; `SessionEnd` in the foreground |

The installer is careful with the user's files (`install/json_file.rs`, `install/hooks.rs`):

- It only installs if the assistant's config directory already exists; otherwise it reports that the assistant is not set up on this computer yet.
- It refuses to edit a file that is larger than 16 MB, not valid JSON, or not a JSON object, and says so, leaving the file untouched. A UTF-8 byte order mark is tolerated.
- It first **strips** any existing notchbase handlers (recognised by the executable name, `notchbase` or the current executable's stem, plus the hook arguments), removing groups and events that become empty, then appends the new groups. Other hooks are never touched. Running it twice changes nothing, and `installed()` checks exactly that.
- Before the first change it keeps a one-time backup, `<file>.notchbase.bak`.
- It writes to a temporary sibling file and renames it into place (retrying briefly if the file is locked), keeps the file's permissions on Unix, preserves a trailing newline, and writes through symlinks to the real file.

After installing hooks for Codex for the first time, the notch says "Open Codex and run /hooks to trust notchbase", because Codex asks the user to trust new hooks.

## Things to keep in mind

- The hook client and the app must agree on the data directory. If you run the app with `NOTCHBASE_DATA_DIR` set, the hooks the assistant runs must see the same variable, or they will not find `assistant.json` and will silently fall back to the assistant's own prompts.
- Hooks point at the executable that installed them. Moving or rebuilding notchbase at another path means reinstalling (toggle the setting off and on); stale handlers are recognised by name and replaced.
- Nothing here needs network access. All traffic stays on `127.0.0.1`.

## Tests

`assistant/tests` covers the board (every signal, holds, sweeping, muting), runs (starting, waiting, finishing, late hooks, going stale, remembering their window, the window a prompt belongs to, ending when that window's process is gone), the wire format (including events from older hooks without a window), the `show` call (`tests/call.rs`: it reaches the app and wakes it, a wrong token is ignored, nothing listening means nothing is sent), and full flows through a real server and client in temporary directories; `tests/child.rs` has ignored tests that run the built `notchbase` binary as a hook process (see [Building and Testing](Building-and-Testing.md#ignored-tests)). `hook/window` tests the ancestor walk (nearest owner, the 12-step limit, loops) and has an ignored test that prints the window owner of the shell running it. `hook/tests.rs` checks payload mapping, descriptions, fingerprints and rendered decisions for both assistants, and `install/tests.rs` checks installing, reinstalling, uninstalling, backups, refusals and command recognition on both Windows-style and Unix-style commands.
