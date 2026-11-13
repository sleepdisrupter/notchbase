//! Asks Anthropic's usage endpoint with the Claude Code login's token: HTTPS only, no redirects, 10 second timeout.

use std::time::{Duration, SystemTime};

use notchbase_net::Mode;
use ureq::http::HeaderMap;

use super::credentials::{self, Credentials, Found};
use super::response::parse_usage;
use super::state::Outcome;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const USER_AGENT: &str = "notchbase";
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY: u64 = 256 * 1024;

pub(super) fn fetch(cached: Option<Credentials>) -> (Outcome, Option<Credentials>) {
    let now = SystemTime::now();
    let (creds, from_cache) = match cached.filter(|creds| !creds.is_expired(now)) {
        Some(creds) => (creds, true),
        None => match credentials::load(now) {
            Found::Valid(creds) => (creds, false),
            Found::Expired => return (Outcome::Expired, None),
            Found::Missing => return (Outcome::SignedOut, None),
            Found::Denied => return (Outcome::KeychainDenied, None),
        },
    };
    match request(creds.access_token()) {
        // The cached token may be out of date, so read the credentials again and try once more.
        Ok(reply) if reply.status == 401 && from_cache => fetch(None),
        Ok(reply) if reply.status == 401 => (Outcome::Expired, None),
        Ok(reply) => (outcome_of(&reply, SystemTime::now()), creds.into_cached()),
        Err(err) => (Outcome::failed(describe(&err)), creds.into_cached()),
    }
}

struct Reply {
    status: u16,
    body: String,
    retry_after: Option<Duration>,
}

fn outcome_of(reply: &Reply, now: SystemTime) -> Outcome {
    match reply.status {
        200 => parse_usage(&reply.body, now)
            .map_or_else(|| Outcome::failed("Unexpected response from Anthropic"), Outcome::Ready),
        403 => Outcome::failed("This login can't read plan usage"),
        429 => Outcome::Failed { message: "Rate limited, try again later".to_owned(), retry_after: reply.retry_after },
        status @ 500.. => Outcome::failed(format!("Anthropic is unavailable (HTTP {status})")),
        status => Outcome::failed(format!("Unexpected response (HTTP {status})")),
    }
}

fn request(token: &str) -> Result<Reply, ureq::Error> {
    let mut response = agent()
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", OAUTH_BETA)
        .header("Accept", "application/json")
        .call()?;
    let status = response.status().as_u16();
    let retry_after = retry_after(response.headers());
    let body =
        if status == 200 { response.body_mut().with_config().limit(MAX_BODY).read_to_string()? } else { String::new() };
    Ok(Reply { status, body, retry_after })
}

fn agent() -> ureq::Agent {
    notchbase_net::agent(TIMEOUT, USER_AGENT, Mode::Exact)
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers.get("retry-after")?.to_str().ok()?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

fn describe(err: &ureq::Error) -> &'static str {
    match err {
        ureq::Error::Timeout(_) => "Request timed out",
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => "Can't reach Anthropic",
        _ => "Couldn't load usage",
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use ureq::http::HeaderValue;

    use super::*;
    use crate::PlanUsage;

    fn reply(status: u16, body: &str, retry_after: Option<Duration>) -> Reply {
        Reply { status, body: body.to_owned(), retry_after }
    }

    fn message(outcome: Outcome) -> String {
        match outcome {
            Outcome::Failed { message, .. } => message,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn statuses_become_outcomes() {
        let now = UNIX_EPOCH + Duration::from_secs(9);
        let ready = outcome_of(&reply(200, r#"{"five_hour": {"utilization": 10}}"#, None), now);
        assert!(matches!(ready, Outcome::Ready(PlanUsage::Ready { fetched_at, .. }) if fetched_at == now));
        assert_eq!(message(outcome_of(&reply(200, "<html>", None), now)), "Unexpected response from Anthropic");
        assert_eq!(message(outcome_of(&reply(403, "", None), now)), "This login can't read plan usage");
        assert_eq!(message(outcome_of(&reply(503, "", None), now)), "Anthropic is unavailable (HTTP 503)");
        assert_eq!(message(outcome_of(&reply(302, "", None), now)), "Unexpected response (HTTP 302)");
        let wait = Some(Duration::from_secs(120));
        assert_eq!(
            outcome_of(&reply(429, "", wait), now),
            Outcome::Failed { message: "Rate limited, try again later".into(), retry_after: wait }
        );
    }

    #[test]
    fn retry_after_takes_whole_seconds() {
        let header = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert("retry-after", HeaderValue::from_static(value));
            headers
        };
        assert_eq!(retry_after(&header(" 90 ")), Some(Duration::from_secs(90)));
        assert_eq!(retry_after(&header("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after(&header("-5")), None);
        assert_eq!(retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn agent_refuses_plain_http() {
        let err = agent().get("http://api.anthropic.com/api/oauth/usage").call().unwrap_err();
        assert!(matches!(err, ureq::Error::RequireHttpsOnly(_)), "{err:?}");
    }

    #[test]
    fn asks_over_https_without_redirects() {
        let agent = agent();
        let config = agent.config();
        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().global, Some(TIMEOUT));
        assert!(
            matches!(config.user_agent(), ureq::config::AutoHeaderValue::Provided(text) if text.as_str() == "notchbase")
        );
    }
}
