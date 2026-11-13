//! A second launch's call to the copy already running, sent on the same port as the hooks.

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::endpoint;
use super::wire::{self, Envelope};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Call {
    pub(super) token: String,
    pub(super) call: Request,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Request {
    Show,
}

/// What the server reads off a line: a hook's event, or a call from a second launch.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Inbound {
    Hook(Envelope),
    Call(Call),
}

impl Inbound {
    pub(super) fn token(&self) -> &str {
        match self {
            Self::Hook(envelope) => &envelope.token,
            Self::Call(call) => &call.token,
        }
    }
}

pub(super) fn send(dir: &Path, request: Request) -> bool {
    let Some(endpoint) = endpoint::read(dir) else {
        return false;
    };
    let Some(line) = wire::encode(&Call { token: endpoint.token, call: request }) else {
        return false;
    };
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) else {
        return false;
    };
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_ok()
        && stream.write_all(&line).and_then(|()| stream.flush()).is_ok()
}
