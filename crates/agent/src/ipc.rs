//! The broker's local socket. Unix only for now.
//!
//! The socket file is created owner-only (0600) in the user's own config
//! directory, and every connection's peer is checked to be the same user.
//! Each connection carries one request line and one response line.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::{ErrorCode, Request, Response};

/// Requests are small; anything larger is not from us.
const MAX_REQUEST: u64 = 1024 * 1024;

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, request: Request) -> Response;
}

/// Binds the broker socket at `path`, replacing a stale one left by a
/// process that's gone. Fails if another broker is already listening.
pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another dotsafe broker is running"));
        }
        std::fs::remove_file(path)?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Serves connections until the listener fails, one thread per connection
/// (a run can wait minutes for approval).
pub fn serve(listener: UnixListener, handler: Arc<dyn Handler>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let handler = handler.clone();
        std::thread::spawn(move || {
            let _ = handle_connection(stream, &*handler);
        });
    }
}

fn handle_connection(stream: UnixStream, handler: &dyn Handler) -> std::io::Result<()> {
    let response = if !same_user(&stream) {
        Response::error(ErrorCode::Denied, "Only the user who owns dotsafe can use it.")
    } else {
        let mut line = String::new();
        BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line)?;
        match serde_json::from_str::<Request>(&line) {
            Ok(request) => handler.handle(request),
            Err(e) => Response::error(ErrorCode::Invalid, format!("Bad request: {e}")),
        }
    };
    let mut stream = stream;
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

/// Sends one request and waits for the response. Runs can wait for the user
/// and then for the command, so the wait is long.
pub fn call(path: &Path, request: &Request) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(20 * 60)))?;
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

/// Whether the process on the other end runs as the same user as us.
fn same_user(stream: &UnixStream) -> bool {
    match peer_uid(stream) {
        Some(uid) => uid == unsafe { libc::geteuid() },
        // Platforms without a peer check rely on the socket's 0600 mode.
        None => true,
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
fn peer_uid(stream: &UnixStream) -> Option<libc::uid_t> {
    use std::os::fd::AsRawFd;
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: the fd is a valid socket for the lifetime of `stream`, and the
    // out-pointers point at live locals.
    let ok = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0;
    ok.then_some(uid)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_uid(stream: &UnixStream) -> Option<libc::uid_t> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are live locals sized for SO_PEERCRED.
    let ok = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    } == 0;
    ok.then_some(cred.uid)
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "linux",
    target_os = "android"
)))]
fn peer_uid(_stream: &UnixStream) -> Option<libc::uid_t> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProjectInfo;

    struct Fake;
    impl Handler for Fake {
        fn handle(&self, request: Request) -> Response {
            match request {
                Request::ListProjects => {
                    Response::Projects(vec![ProjectInfo { name: "api".into(), environments: vec!["dev".into()] }])
                }
                _ => Response::error(ErrorCode::Invalid, "unsupported"),
            }
        }
    }

    #[test]
    fn round_trip_over_an_owner_only_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.sock");
        let listener = bind(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::thread::spawn(move || serve(listener, Arc::new(Fake)));

        match call(&path, &Request::ListProjects).unwrap() {
            Response::Projects(p) => assert_eq!(p[0].name, "api"),
            other => panic!("{other:?}"),
        }
        assert!(bind(&path).is_err(), "a second broker can't take over a live socket");
    }

    #[test]
    fn stale_sockets_are_replaced() {
        // A leftover file nothing listens on, as after a crash. (Dropping a
        // real listener here would race with other tests' child processes,
        // which can briefly inherit it on macOS.)
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.sock");
        std::fs::write(&path, b"").unwrap();
        bind(&path).unwrap();
    }
}
