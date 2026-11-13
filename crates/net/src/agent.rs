//! One place to build HTTPS agents, so every request trusts the platform's certificates.

use std::time::Duration;

use ureq::Agent;
use ureq::config::ConfigBuilder;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::typestate::AgentScope;

#[macro_export]
macro_rules! user_agent {
    () => {
        concat!("notchbase/", env!("CARGO_PKG_VERSION"))
    };
}

// Follow redirects and treat error statuses as errors, or hand back every response as it came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Follow,
    Exact,
}

pub fn config(timeout: Duration) -> ConfigBuilder<AgentScope> {
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    Agent::config_builder().timeout_global(Some(timeout)).tls_config(tls)
}

pub fn agent(timeout: Duration, user_agent: &str, mode: Mode) -> Agent {
    let config = config(timeout).https_only(true).user_agent(user_agent);
    let config = match mode {
        Mode::Follow => config,
        Mode::Exact => config.max_redirects(0).http_status_as_error(false),
    };
    config.build().into()
}

#[cfg(test)]
mod tests {
    use ureq::config::AutoHeaderValue;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(7);
    const AGENT: &str = "notchbase/9.9.9";

    fn defaults() -> ureq::config::Config {
        Agent::config_builder().build()
    }

    fn provided(value: &AutoHeaderValue) -> Option<&str> {
        match value {
            AutoHeaderValue::Provided(text) => Some(text.as_str()),
            _ => None,
        }
    }

    #[test]
    fn user_agent_names_the_version() {
        let text = user_agent!();
        assert_eq!(text, format!("notchbase/{}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn config_uses_native_tls() {
        let built = config(TIMEOUT).build();
        assert_eq!(built.tls_config().provider(), TlsProvider::NativeTls);
        assert!(matches!(built.tls_config().root_certs(), RootCerts::PlatformVerifier));
        assert_eq!(built.timeouts().global, Some(TIMEOUT));
    }

    #[test]
    fn config_keeps_ureq_defaults() {
        let built = config(TIMEOUT).build();
        let defaults = defaults();
        assert!(!built.https_only());
        assert_eq!(built.max_redirects(), defaults.max_redirects());
        assert_eq!(built.http_status_as_error(), defaults.http_status_as_error());
        assert!(matches!(built.user_agent(), AutoHeaderValue::Default));
    }

    #[test]
    fn agent_is_https_only() {
        for mode in [Mode::Follow, Mode::Exact] {
            let agent = agent(TIMEOUT, AGENT, mode);
            let config = agent.config();
            assert!(config.https_only());
            assert_eq!(provided(config.user_agent()), Some(AGENT));
            assert_eq!(config.timeouts().global, Some(TIMEOUT));
            assert_eq!(config.tls_config().provider(), TlsProvider::NativeTls);
            assert!(matches!(config.tls_config().root_certs(), RootCerts::PlatformVerifier));
        }
    }

    #[test]
    fn follow_mode_keeps_redirects() {
        let agent = agent(TIMEOUT, AGENT, Mode::Follow);
        let defaults = defaults();
        assert_eq!(agent.config().max_redirects(), defaults.max_redirects());
        assert!(agent.config().max_redirects() > 0);
        assert!(agent.config().http_status_as_error());
    }

    #[test]
    fn exact_mode_returns_every_status() {
        let agent = agent(TIMEOUT, AGENT, Mode::Exact);
        assert_eq!(agent.config().max_redirects(), 0);
        assert!(!agent.config().http_status_as_error());
    }

    #[test]
    fn plain_http_is_refused() {
        for mode in [Mode::Follow, Mode::Exact] {
            let refused = agent(TIMEOUT, AGENT, mode).get("http://example.invalid/").call();
            assert!(matches!(refused, Err(ureq::Error::RequireHttpsOnly(_))), "{mode:?}");
        }
    }

    #[test]
    fn empty_user_agent_sends_none() {
        let agent = agent(TIMEOUT, "", Mode::Follow);
        assert!(matches!(agent.config().user_agent(), AutoHeaderValue::None));
    }

    #[test]
    fn zero_timeout_is_kept() {
        let agent = agent(Duration::ZERO, AGENT, Mode::Follow);
        assert_eq!(agent.config().timeouts().global, Some(Duration::ZERO));
    }
}
