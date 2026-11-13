//! Sends one event line to the server, and for an Ask waits for the answer and renders the decision.

use std::io::{BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::Plan;
use crate::assistant::endpoint::Endpoint;
use crate::assistant::timing;
use crate::assistant::wire::{self, Answer, Envelope};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn deliver(endpoint: &Endpoint, plan: Plan) -> Option<String> {
    let Plan { event, render } = plan;
    let holds = event.signal.holds();
    let line = wire::encode(&Envelope { token: endpoint.token.clone(), event })?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).ok()?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok()?;
    stream.write_all(&line).and_then(|()| stream.flush()).ok()?;
    if !holds {
        return None;
    }
    stream.set_read_timeout(Some(timing::patience())).ok()?;
    let answer: Answer = wire::decode(&mut BufReader::new(&stream), wire::MAX_ANSWER)?;
    render.output(&answer)
}
