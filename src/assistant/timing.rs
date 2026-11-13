//! How long things wait: a held prompt 4 hours in Claude, 25 seconds in a subagent and 5 minutes in Codex.

use std::time::Duration;

use super::prompt::Provider;

const CLAUDE_HOLD: Duration = Duration::from_secs(4 * 60 * 60);
const CLAUDE_AGENT_HOLD: Duration = Duration::from_secs(25);
const CODEX_HOLD: Duration = Duration::from_secs(5 * 60);
const HOOK_GRACE: Duration = Duration::from_secs(60);

pub(super) const FINISHED_TTL: Duration = Duration::from_secs(10 * 60);
pub(super) const ATTENTION_TTL: Duration = Duration::from_secs(15 * 60);
pub(super) const NOTE_TTL: Duration = Duration::from_secs(30 * 60);

pub(super) fn hold(provider: Provider, in_agent: bool) -> Duration {
    match (provider, in_agent) {
        (Provider::Claude, false) => CLAUDE_HOLD,
        (Provider::Claude, true) => CLAUDE_AGENT_HOLD,
        (Provider::Codex, _) => CODEX_HOLD,
    }
}

/// The hook's timeout in the assistant's config: the hold plus a minute, so we always answer first.
pub(super) fn hook_timeout_secs(provider: Provider) -> u64 {
    (hold(provider, false) + HOOK_GRACE).as_secs()
}

/// How long the hook client waits for an answer, a little past the longest hold.
pub(super) fn patience() -> Duration {
    CLAUDE_HOLD.max(CODEX_HOLD) + HOOK_GRACE * 2
}
