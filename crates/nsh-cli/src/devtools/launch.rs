//! Starts an app's host and connects to its DevTools endpoint.

use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant},
};

use nsh_devtools::client::{Client, ClientError};

use crate::{test_host::indented, ui};

/// What [`launch`] starts and how long it waits for it.
pub(crate) struct LaunchSpec {
    /// The host, with its arguments and environment (the endpoint's
    /// variables included).
    pub(crate) command: Command,
    /// Where the host writes its endpoint's URL.
    pub(crate) port_file: PathBuf,
    /// The token the host's endpoint was given.
    pub(crate) token: String,
    /// How long the host may take to announce itself and accept the
    /// connection.
    pub(crate) connect_timeout: Duration,
}

/// What ends the host with the launcher: on Windows a job object that kills
/// its processes when it closes (with this process, however it ends); the
/// other systems need nothing (the host's own endpoint watches its client).
#[cfg(windows)]
pub(crate) type Job = Option<crate::run::KillOnClose>;
/// What ends the host with the launcher (nothing off Windows).
#[cfg(not(windows))]
pub(crate) type Job = ();

/// A host running, and the connection to its endpoint.
pub(crate) struct Launched {
    pub(crate) child: Child,
    /// Keeps the host in its job; closing it (dropping this) ends the host.
    pub(crate) job: Job,
    /// The host's last output lines.
    pub(crate) output: ui::Tail,
    pub(crate) client: Client,
    /// The endpoint's URL, which another client connects to.
    pub(crate) url: String,
    /// The token the endpoint was given.
    pub(crate) token: String,
}

/// Starts the host and connects to the endpoint it announces in the port
/// file.
///
/// The port file is deleted before the host starts: one an earlier run left
/// names an endpoint that is not this host's.
pub(crate) fn launch(mut spec: LaunchSpec) -> Result<Launched, String> {
    match std::fs::remove_file(&spec.port_file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!("cannot remove {}: {e}", spec.port_file.display()));
        }
    }
    let (mut child, output) = ui::quiet(&mut spec.command).map_err(|e| {
        format!(
            "cannot start {}: {e}",
            spec.command.get_program().to_string_lossy()
        )
    })?;
    // Right after the spawn: the host dies with the launcher, also when the
    // launcher is killed. A host that crashes (a fail-fast, 0xc0000409) dies
    // at once instead of staying in Windows Error Reporting, where closing
    // the job cannot end it.
    #[cfg(windows)]
    let job = crate::run::KillOnClose::assign_with(
        &child,
        windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    );
    #[cfg(not(windows))]
    let job = ();
    let deadline = Instant::now() + spec.connect_timeout;
    let fail = |child: &mut Child, why: String, output: &ui::Tail| {
        let _ = child.kill();
        let _ = child.wait();
        // The output thread may still be reading the last lines.
        std::thread::sleep(Duration::from_millis(100));
        format!("{why}{}", indented(&output.text()))
    };
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            // The output thread may still be reading the last lines.
            std::thread::sleep(Duration::from_millis(100));
            let status = match status.code() {
                Some(code) => format!("exit status {code}"),
                None => status.to_string(),
            };
            return Err(format!(
                "the host exited ({status}) before it announced its DevTools endpoint{}",
                indented(&output.text())
            ));
        }
        let url = std::fs::read_to_string(&spec.port_file)
            .map(|t| t.trim().to_owned())
            .unwrap_or_default();
        if !url.is_empty() {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(200));
            match Client::connect(&url, &spec.token, remaining) {
                Ok(client) => {
                    return Ok(Launched {
                        child,
                        job,
                        output,
                        client,
                        url,
                        token: spec.token.clone(),
                    });
                }
                // The file may be read half written, or the endpoint not
                // yet accepting: try again until the deadline.
                Err(ClientError::BadUrl(_) | ClientError::Connect(_)) => {}
                Err(e) => return Err(fail(&mut child, e.to_string(), &output)),
            }
        }
        if Instant::now() >= deadline {
            let why = format!(
                "the host did not announce its DevTools endpoint in {} s",
                spec.connect_timeout.as_secs_f32().ceil()
            );
            return Err(fail(&mut child, why, &output));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A token for one host: 32 hex characters, unguessable by other local
/// users only as far as the std's per-process random hash keys go, which is
/// what a loopback endpoint of one test run needs.
pub(crate) fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(i);
        hasher.write_u32(std::process::id());
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

#[cfg(test)]
#[cfg(unix)]
mod devtools_launch {
    use super::*;

    fn spec(script: &str, port_file: PathBuf, timeout: Duration) -> LaunchSpec {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        LaunchSpec {
            command,
            port_file,
            token: "t".into(),
            connect_timeout: timeout,
        }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nsh-devtools-launch-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_stale_port_file_is_never_read() {
        let port_file = temp("stale.port");
        std::fs::write(&port_file, "ws://127.0.0.1:1/devtools").unwrap();
        let err = launch(spec(
            "sleep 5",
            port_file.clone(),
            Duration::from_millis(500),
        ))
        .err()
        .expect("an error");
        assert!(err.contains("did not announce"), "{err}");
        assert!(!err.contains("cannot connect"), "{err}");
        let _ = std::fs::remove_file(&port_file);
    }

    #[test]
    fn an_app_that_exits_early_is_reported() {
        let start = Instant::now();
        let err = launch(spec(
            "echo boom >&2; exit 3",
            temp("early.port"),
            Duration::from_secs(30),
        ))
        .err()
        .expect("an error");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
        assert!(err.contains("exit status 3"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }
}
