//! The broker's local socket. Unix only for now.
//!
//! The socket file is created owner-only (0600) in the user's own config
//! directory, and every connection's peer is checked to be the same user.
//! Each connection carries one request line and one response line.
//!
//! An `exec` request (`dotsafe run` through the daemon) also carries the
//! caller's stdin, stdout and stderr as file descriptors (`SCM_RIGHTS`), so the
//! command the broker starts writes straight to the caller's terminal or log.
//! The connection stays open while it runs; closing it stops the command.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::protocol::{ErrorCode, ExecRequest, Request, Response};

/// Requests are small; anything larger is not from us.
const MAX_REQUEST: usize = 1024 * 1024;
/// Descriptors an `exec` passes: stdin, stdout, stderr.
const EXEC_FDS: usize = 3;
/// Room for a few more than that, so extras are seen (and closed), not lost.
const MAX_FDS: usize = 8;
#[cfg(any(target_os = "linux", target_os = "android"))]
const RECV_FLAGS: libc::c_int = libc::MSG_CMSG_CLOEXEC;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const RECV_FLAGS: libc::c_int = 0;

pub trait Handler: Send + Sync + 'static {
    fn handle(&self, request: Request) -> Response;

    /// Runs a command on the caller's own stdio. Only `dotsafe daemon` does.
    fn exec(&self, _request: ExecRequest, _caller: Caller) -> Response {
        Response::error(ErrorCode::Unavailable, "This broker doesn't run commands for dotsafe run.")
    }
}

/// The other end of an `exec`: its stdio, and the connection, to notice when
/// it goes away.
pub struct Caller {
    pub stdin: OwnedFd,
    pub stdout: OwnedFd,
    pub stderr: OwnedFd,
    stream: UnixStream,
}

impl Caller {
    /// Whether the caller hung up (it was killed, or gave up).
    pub fn gone(&self) -> bool {
        let mut byte = 0u8;
        // SAFETY: the fd is a live socket owned by `self.stream`, and the
        // buffer is a live local of the given length.
        let n = unsafe {
            libc::recv(
                self.stream.as_raw_fd(),
                (&mut byte as *mut u8).cast(),
                1,
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        match n {
            0 => true,
            n if n > 0 => false,
            _ => {
                let err = std::io::Error::last_os_error();
                !matches!(err.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted)
            }
        }
    }
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
        let (line, mut fds) = read_request(&stream)?;
        match serde_json::from_slice::<Request>(&line) {
            Ok(Request::Exec(request)) if fds.len() == EXEC_FDS => {
                let stderr = fds.pop().expect("three fds");
                let stdout = fds.pop().expect("three fds");
                let stdin = fds.pop().expect("three fds");
                let caller = Caller { stdin, stdout, stderr, stream: stream.try_clone()? };
                handler.exec(request, caller)
            }
            Ok(Request::Exec(_)) => Response::error(ErrorCode::Invalid, "dotsafe run sends its stdin, stdout and stderr."),
            // Descriptors on anything else are dropped (closed) here.
            Ok(request) => handler.handle(request),
            Err(e) => Response::error(ErrorCode::Invalid, format!("Bad request: {e}")),
        }
    };
    let mut stream = stream;
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

/// Reads the request line, and any descriptors sent with it.
fn read_request(stream: &UnixStream) -> std::io::Result<(Vec<u8>, Vec<OwnedFd>)> {
    let mut line = Vec::new();
    let mut fds = Vec::new();
    let mut buf = [0u8; 8192];
    // u64 keeps the control buffer aligned for `cmsghdr`.
    let mut control = [0u64; 32];
    loop {
        let mut iov = libc::iovec { iov_base: buf.as_mut_ptr().cast(), iov_len: buf.len() };
        // SAFETY: an all-zero msghdr is valid; the pointers set below point at
        // live locals that outlive the call.
        let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = std::mem::size_of_val(&control) as _;
        // SAFETY: `msg` describes valid buffers, and the fd is a live socket.
        let n = unsafe { libc::recvmsg(stream.as_raw_fd(), &mut msg, RECV_FLAGS) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        // SAFETY: the kernel filled `msg`'s control buffer; the CMSG macros
        // walk only within `msg_controllen`.
        unsafe {
            let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
            while !cmsg.is_null() {
                if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                    let data = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                    let count = ((*cmsg).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / std::mem::size_of::<RawFd>();
                    for i in 0..count {
                        let fd = std::ptr::read_unaligned(data.add(i));
                        // Not inherited by other commands the broker starts
                        // (Linux already set it, atomically).
                        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                        fds.push(OwnedFd::from_raw_fd(fd));
                    }
                }
                cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
            }
        }
        if msg.msg_flags & libc::MSG_CTRUNC != 0 || fds.len() > MAX_FDS {
            return Err(std::io::Error::other("too many descriptors"));
        }
        let n = n as usize;
        line.extend_from_slice(&buf[..n]);
        if n == 0 || buf[..n].contains(&b'\n') {
            break;
        }
        if line.len() > MAX_REQUEST {
            return Err(std::io::Error::other("request too large"));
        }
    }
    if let Some(end) = line.iter().position(|b| *b == b'\n') {
        line.truncate(end);
    }
    Ok((line, fds))
}

/// Sends one request and waits for the response. Runs can wait for the user
/// and then for the command, so the wait is long.
pub fn call(path: &Path, request: &Request) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(20 * 60)))?;
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    read_response(&stream)
}

/// Sends an `exec` with this process's stdin, stdout and stderr, and waits for
/// the command to finish, however long it runs.
pub fn call_exec(path: &Path, request: ExecRequest, stdio: [BorrowedFd<'_>; 3]) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    let mut bytes = serde_json::to_vec(&Request::Exec(request))?;
    bytes.push(b'\n');
    let sent = send_with_fds(&stream, &bytes, &stdio.map(|fd| fd.as_raw_fd()))?;
    stream.write_all(&bytes[sent..])?;
    read_response(&stream)
}

fn read_response(stream: &UnixStream) -> std::io::Result<Response> {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(std::io::Error::other)
}

/// Sends the start of `bytes` with `fds` attached. Returns how much was sent.
fn send_with_fds(stream: &UnixStream, bytes: &[u8], fds: &[RawFd]) -> std::io::Result<usize> {
    let mut control = [0u64; 8];
    let fd_bytes = std::mem::size_of_val(fds);
    // SAFETY: CMSG_SPACE only computes a size.
    let space = unsafe { libc::CMSG_SPACE(fd_bytes as u32) } as usize;
    assert!(space <= std::mem::size_of_val(&control), "control buffer too small");
    let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut _, iov_len: bytes.len() };
    // SAFETY: as in `read_request`; every pointer is to a live local, and the
    // header and fds are written within the control buffer's `space` bytes.
    let n = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(fd_bytes as u32) as _;
        std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(cmsg), fd_bytes);
        libc::sendmsg(stream.as_raw_fd(), &msg, 0)
    };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(n as usize)
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

        /// Writes to the caller's stdout, then waits for it to hang up.
        fn exec(&self, request: ExecRequest, caller: Caller) -> Response {
            let mut out = std::fs::File::from(caller.stdout.try_clone().unwrap());
            writeln!(out, "ran {}", request.command.join(" ")).unwrap();
            drop(out);
            assert!(!caller.gone());
            Response::Exited(crate::protocol::ExitInfo { code: Some(7), signal: None })
        }
    }

    #[test]
    fn exec_passes_the_callers_stdio() {
        use std::io::Read;
        use std::os::fd::AsFd;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.sock");
        let listener = bind(&path).unwrap();
        std::thread::spawn(move || serve(listener, Arc::new(Fake)));

        // The handler writes into this file through the descriptor it got.
        let out_path = dir.path().join("out");
        let out = std::fs::File::create(&out_path).unwrap();
        let null = std::fs::File::open("/dev/null").unwrap();
        let request = ExecRequest {
            client: "test".into(),
            command: vec!["job".into(), "--nightly".into()],
            cwd: dir.path().into(),
            target: Default::default(),
            variables: None,
            path: None,
            proxy: false,
            wait_secs: None,
        };
        let response = call_exec(&path, request.clone(), [null.as_fd(), out.as_fd(), out.as_fd()]).unwrap();
        assert_eq!(response, Response::Exited(crate::protocol::ExitInfo { code: Some(7), signal: None }));
        let mut text = String::new();
        std::fs::File::open(&out_path).unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "ran job --nightly\n");

        // Without descriptors, exec is refused.
        match call(&path, &Request::Exec(request)).unwrap() {
            Response::Error(e) => assert_eq!(e.code, ErrorCode::Invalid),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_caller_that_hangs_up_is_gone() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let fd = || OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
        let caller = Caller { stdin: fd(), stdout: fd(), stderr: fd(), stream: ours };
        assert!(!caller.gone());
        drop(theirs);
        assert!(caller.gone());
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
