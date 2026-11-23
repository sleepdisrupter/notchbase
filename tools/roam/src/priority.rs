//! Puts Roam's packets at the front of the Wi-Fi queue, the way calls and game streams do.
//!
//! Wi-Fi keeps four queues: voice, video, ordinary and background. A packet in the voice queue
//! goes out first and waits the shortest for the air, so the mouse and keyboard go there.
//!
//! Each system wants its own way of saying so:
//! - Windows ignores the type-of-service byte an ordinary program sets on its packets. Only its
//!   QoS API (qWAVE) marks them, per destination for a socket that isn't connected.
//! - macOS picks the Wi-Fi queue from the socket's service type; the type-of-service byte then
//!   also tells the router.
//! - Elsewhere the type-of-service byte is all there is.

use std::net::SocketAddr;

/// What a socket carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Traffic {
    /// The mouse and keyboard: small, and wanted at once.
    Voice,
}

impl Traffic {
    /// The type-of-service byte: expedited forwarding for voice.
    pub fn tos(self) -> u32 {
        match self {
            Traffic::Voice => 0xB8,
        }
    }
}

/// Marks what `socket` sends to `to` (`None` for a connected socket) as `traffic`. Answers a
/// line for the log the first time it works or fails for that kind of traffic, else `None`.
///
/// A socket that isn't connected is marked once for each destination where the system wants
/// that (Windows), else once: call this before each send, and repeats cost a lookup.
pub fn mark(socket: socket2::SockRef<'_>, to: Option<SocketAddr>, traffic: Traffic) -> Option<String> {
    if let Some(to) = to
        && !fresh(raw(&socket), platform::PER_DESTINATION.then_some(to))
    {
        return None;
    }
    let _ = socket.set_tos_v4(traffic.tos());
    platform::mark(&socket, to, traffic)
}

#[cfg(windows)]
fn raw(socket: &socket2::SockRef<'_>) -> u64 {
    use std::os::windows::io::AsRawSocket;
    socket.as_raw_socket()
}

#[cfg(unix)]
fn raw(socket: &socket2::SockRef<'_>) -> u64 {
    use std::os::fd::AsRawFd;
    socket.as_raw_fd() as u64
}

/// Whether this socket (and destination) wasn't marked yet, noting that it now is.
fn fresh(socket: u64, to: Option<SocketAddr>) -> bool {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    type Marked = Mutex<HashSet<(u64, Option<SocketAddr>)>>;
    static MARKED: OnceLock<Marked> = OnceLock::new();
    let mut marked = MARKED.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap_or_else(|e| e.into_inner());
    // Systems hand socket numbers out again: keep the list from growing for good.
    if marked.len() > 4096 {
        marked.clear();
    }
    marked.insert((socket, to))
}

/// Says once per kind of traffic how marking went.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn first(traffic: Traffic, result: Result<(), String>) -> Option<String> {
    use std::sync::Mutex;
    static TOLD: Mutex<Vec<Traffic>> = Mutex::new(Vec::new());
    let mut told = TOLD.lock().unwrap_or_else(|e| e.into_inner());
    if told.contains(&traffic) {
        return None;
    }
    told.push(traffic);
    let what = match traffic {
        Traffic::Voice => "the mouse and keyboard",
    };
    Some(match result {
        Ok(()) => format!("Wi-Fi priority for {what}: on"),
        Err(why) => format!("Wi-Fi priority for {what}: couldn't be set ({why})"),
    })
}

#[cfg(windows)]
mod platform {
    use std::net::SocketAddr;
    use std::os::windows::io::AsRawSocket;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
    use windows_sys::Win32::NetworkManagement::QoS::{
        QOS_NON_ADAPTIVE_FLOW, QOS_TRAFFIC_TYPE, QOS_VERSION, QOSAddSocketToFlow, QOSCreateHandle, QOSTrafficTypeVoice,
    };
    use windows_sys::Win32::Networking::WinSock::{SOCKADDR, SOCKET};

    use super::{Traffic, first};

    /// An unconnected socket gets a flow for each place it sends to.
    pub(super) const PER_DESTINATION: bool = true;

    /// The QoS handle, made once and kept: closing it would end every flow.
    fn handle() -> Result<usize, String> {
        static HANDLE_: OnceLock<Result<usize, String>> = OnceLock::new();
        HANDLE_
            .get_or_init(|| {
                let version = QOS_VERSION { MajorVersion: 1, MinorVersion: 0 };
                let mut handle: HANDLE = std::ptr::null_mut();
                // SAFETY: a version and a place for the handle, as the call expects.
                if unsafe { QOSCreateHandle(&version, &mut handle) } == 0 || handle.is_null() {
                    // SAFETY: plain query.
                    return Err(format!("QoS isn't available, error {}", unsafe { GetLastError() }));
                }
                Ok(handle as usize)
            })
            .clone()
    }

    pub(super) fn mark(socket: &socket2::SockRef<'_>, to: Option<SocketAddr>, traffic: Traffic) -> Option<String> {
        let raw = socket.as_raw_socket();
        let result = handle().and_then(|handle| {
            let kind: QOS_TRAFFIC_TYPE = match traffic {
                Traffic::Voice => QOSTrafficTypeVoice,
            };
            let address = to.map(socket2::SockAddr::from);
            let destination = address.as_ref().map_or(std::ptr::null(), |a| a.as_ptr().cast::<SOCKADDR>());
            // SAFETY: a live socket, a destination that outlives the call (or none for a
            // connected socket), and a flow id starting at 0 as the call requires. First as an
            // adaptive flow, which also learns the path; a network that can't be probed refuses
            // that (error 50, not supported), and a plain flow, as video calls use, still tags the
            // packets for Wi-Fi's queues.
            for flags in [0, QOS_NON_ADAPTIVE_FLOW] {
                let mut flow = 0u32;
                if unsafe { QOSAddSocketToFlow(handle as HANDLE, raw as SOCKET, destination, kind, flags, &mut flow) }
                    != 0
                {
                    return Ok(());
                }
            }
            // SAFETY: plain query.
            Err(format!("error {}", unsafe { GetLastError() }))
        });
        first(traffic, result)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::net::SocketAddr;
    use std::os::fd::AsRawFd;

    use super::{Traffic, first};

    pub(super) const PER_DESTINATION: bool = false;

    const SOL_SOCKET: i32 = 0xffff;
    const SO_NET_SERVICE_TYPE: i32 = 0x1116;
    const NET_SERVICE_TYPE_VO: i32 = 4;

    unsafe extern "C" {
        fn setsockopt(socket: i32, level: i32, name: i32, value: *const std::ffi::c_void, length: u32) -> i32;
    }

    pub(super) fn mark(socket: &socket2::SockRef<'_>, _to: Option<SocketAddr>, traffic: Traffic) -> Option<String> {
        let kind = match traffic {
            Traffic::Voice => NET_SERVICE_TYPE_VO,
        };
        // SAFETY: an int option on a live socket.
        let done = unsafe {
            setsockopt(
                socket.as_raw_fd(),
                SOL_SOCKET,
                SO_NET_SERVICE_TYPE,
                (&kind as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        };
        let result = if done == 0 { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) };
        first(traffic, result)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::net::SocketAddr;

    use super::Traffic;

    pub(super) const PER_DESTINATION: bool = false;

    pub(super) fn mark(_socket: &socket2::SockRef<'_>, _to: Option<SocketAddr>, _traffic: Traffic) -> Option<String> {
        None
    }
}
