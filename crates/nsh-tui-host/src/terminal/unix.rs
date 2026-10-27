//! The host, rather than the event parser, owns terminal lifetime and I/O waits.

use std::{
    io::{self, IsTerminal},
    time::Duration,
};

const OUTPUT_WAIT: Duration = Duration::from_millis(16);

pub(super) struct Transport {
    flags: Vec<(libc::c_int, libc::c_int)>,
}

impl Transport {
    pub(super) fn enter() -> io::Result<Self> {
        if !io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "interactive terminal input is closed or redirected; use --headless for tests",
            ));
        }
        let mut transport = Self { flags: Vec::new() };
        // Capture both before changing either: inherited stdin and stdout may share an open file description.
        for fd in [libc::STDIN_FILENO, libc::STDOUT_FILENO] {
            // SAFETY: fcntl checks a process-owned standard descriptor without dereferencing memory.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 {
                return Err(io::Error::last_os_error());
            }
            transport.flags.push((fd, flags));
        }
        for &(fd, flags) in &transport.flags {
            // SAFETY: these descriptors and their saved flags were checked above.
            if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(transport)
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        for &(fd, flags) in &self.flags {
            // SAFETY: restores the original flags of a standard descriptor owned by the host.
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, flags);
            }
        }
    }
}

pub(super) fn disconnected() -> io::Result<bool> {
    if !io::stdin().is_terminal() {
        return Ok(true);
    }
    let mut fds = [libc::STDIN_FILENO, libc::STDOUT_FILENO].map(|fd| libc::pollfd {
        fd,
        events: 0,
        revents: 0,
    });
    // SAFETY: fds points to two initialized pollfd values. This is a nonblocking lifetime check.
    if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fds
        .iter()
        .any(|fd| fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0))
}

pub(super) fn write(bytes: &[u8]) -> io::Result<usize> {
    loop {
        if crate::shutdown::requested() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "terminal host is shutting down",
            ));
        }
        // SAFETY: bytes is readable for its length; stdout is nonblocking for the session's lifetime.
        let written =
            unsafe { libc::write(libc::STDOUT_FILENO, bytes.as_ptr().cast(), bytes.len()) };
        if written >= 0 {
            return Ok(written as usize);
        }
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::Interrupted => continue,
            io::ErrorKind::WouldBlock => {
                let mut fd = libc::pollfd {
                    fd: libc::STDOUT_FILENO,
                    events: libc::POLLOUT,
                    revents: 0,
                };
                // SAFETY: fd is initialized and remains live for this bounded wait.
                let ready =
                    unsafe { libc::poll(&mut fd, 1, OUTPUT_WAIT.as_millis() as libc::c_int) };
                if ready < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::Interrupted {
                        return Err(error);
                    }
                } else if fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "terminal output disconnected",
                    ));
                }
            }
            _ => return Err(error),
        }
    }
}

pub(super) fn restore(mut bytes: &[u8]) {
    // Cleanup is best-effort even on a full/disconnected PTY. Never wait for output here.
    while !bytes.is_empty() {
        // SAFETY: bytes is live; stdout remains nonblocking until the transport guard drops.
        let written =
            unsafe { libc::write(libc::STDOUT_FILENO, bytes.as_ptr().cast(), bytes.len()) };
        if written <= 0 {
            break;
        }
        bytes = &bytes[written as usize..];
    }
}
