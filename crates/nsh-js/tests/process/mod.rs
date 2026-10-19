//! Offline `process` module tests: streaming, writable stdin, the child's
//! environment and VM lifetime.
#![cfg(target_os = "linux")]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nsh_core::{
    Op,
    access::{Access, Capabilities},
    platform::{Environment, Os},
};
use nsh_js::Host;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    host: Host,
    dir: PathBuf,
    wakes: Arc<AtomicUsize>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                    .join(".cache/nsh-benchs-test-tmp")
            });
        assert!(
            !root.starts_with("/tmp"),
            "test TMPDIR must be outside /tmp"
        );
        let dir = root.join(format!(
            "native-process-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let host = Host::new().unwrap();
        host.set_environment(&Environment {
            os: Os::Linux,
            ..Default::default()
        });
        let caps = Capabilities::from_json(&serde_json::json!({
            "process": { "execute": ["/bin/sh", "sh", "/nonexistent/nsh-fixture"] },
            "fs": { "read": ["${dataDir}"], "write": ["${dataDir}"] }
        }))
        .unwrap();
        host.set_system_access(
            Access {
                declared: caps,
                granted: None,
            },
            Some(dir.clone()),
        );
        let wakes = Arc::new(AtomicUsize::new(0));
        let signal = wakes.clone();
        host.set_system_wake(Arc::new(move || {
            signal.fetch_add(1, Ordering::Relaxed);
        }));
        Self { host, dir, wakes }
    }
    fn complete(&self) {
        let until = Instant::now() + Duration::from_secs(5);
        while self.host.has_system_tasks() {
            assert!(Instant::now() < until, "native process did not finish");
            self.host.run_system();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.host.run_system();
    }
    fn outputs(&self) -> Vec<String> {
        let mut ops = Vec::new();
        self.host.take_ops(&mut ops);
        ops.into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const REPORT: &str = "import { text as report } from 'nsh:rt'; import process from 'process';";

#[test]
fn output_arrives_before_stdin_roundtrip_and_exit_is_last() {
    let f = Fixture::new();
    f.host.eval_source(&format!(r#"{REPORT}
      globalThis.events = [];
      globalThis.child = process.spawn('/bin/sh', ['-c', 'printf ready; read line; printf "%s" "$line"; printf err >&2'], {{stdin: 'pipe'}});
      child.on('stdout', text => {{ events.push(['out', text]); report(text); }});
      child.on('stderr', text => events.push(['err', text]));
      child.on('exit', result => {{ events.push(['exit', result.code]); report(JSON.stringify(events)); }});
    "#)).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut first = Vec::new();
    while first.is_empty() {
        assert!(Instant::now() < until);
        f.host.run_system();
        first.extend(f.outputs());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(first, ["ready"]);
    assert!(
        f.host.has_system_tasks(),
        "child must still be awaiting stdin"
    );
    f.host
        .eval_source("child.write('roundtrip\\n'); child.end();")
        .unwrap();
    f.complete();
    let output = f.outputs();
    let events: serde_json::Value = serde_json::from_str(output.last().unwrap()).unwrap();
    assert_eq!(
        events.as_array().unwrap().last().unwrap(),
        &serde_json::json!(["exit", 0])
    );
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == &serde_json::json!(["out", "roundtrip"]))
    );
    assert!(
        events
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == &serde_json::json!(["err", "err"]))
    );
    assert!(f.wakes.load(Ordering::Relaxed) > 0);
}

#[test]
fn run_preserves_split_utf8_and_captures_both_pipes() {
    let f = Fixture::new();
    f.host.eval_source(&format!(r#"{REPORT}
      process.run('/bin/sh', ['-c', "printf '\\342'; sleep 0.03; printf '\\202\\254'; printf warning >&2"])
        .then(r => report(JSON.stringify(r)));
    "#)).unwrap();
    f.complete();
    let outputs = f.outputs();
    let result: serde_json::Value = serde_json::from_str(outputs.last().unwrap()).unwrap();
    assert_eq!(result["stdout"], "€");
    assert_eq!(result["stderr"], "warning");
    assert_eq!(result["code"], 0);
}

#[test]
fn capability_and_launch_failures_are_separate_from_exit() {
    let f = Fixture::new();
    f.host
        .eval_source(&format!(
            r#"{REPORT}
      for (const command of ['definitely-not-granted', '/nonexistent/nsh-fixture']) {{
        try {{ process.spawn(command); report('unexpected'); }} catch (e) {{ report(String(e)); }}
      }}
    "#
        ))
        .unwrap();
    let output = f.outputs();
    assert!(output[0].contains("capability denied"));
    assert!(!output[1].contains("capability denied"));
    assert!(!f.host.has_system_tasks());
}

#[test]
fn kill_terminates_descendants_and_emits_one_exit_after_output() {
    let f = Fixture::new();
    let pidfile = f.dir.join("descendant.pid");
    let script = format!(
        "sleep 30 & echo $! > '{}'; printf ready; wait",
        pidfile.display()
    );
    let script = serde_json::to_string(&script).unwrap();
    f.host
        .eval_source(&format!(
            r#"{REPORT}
      globalThis.child = process.spawn('/bin/sh', ['-c', {script}]);
      child.on('stdout', t => report(t));
      child.on('exit', r => report(JSON.stringify(r)));
    "#
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut outputs = Vec::new();
    while outputs.is_empty() {
        assert!(Instant::now() < until);
        f.host.run_system();
        outputs.extend(f.outputs());
        std::thread::sleep(Duration::from_millis(5));
    }
    f.host.eval_source("child.kill();").unwrap();
    f.complete();
    outputs.extend(f.outputs());
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0], "ready");
    let result: serde_json::Value = serde_json::from_str(&outputs[1]).unwrap();
    assert_eq!(result["code"], serde_json::Value::Null);
    let pid: u32 = std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_process_stopped(pid);
}

fn assert_process_stopped(pid: u32) {
    // A killed orphan can remain as a zombie until the container's init reaps it.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
    assert!(stat.is_err() || stat.unwrap().split(") ").nth(1).unwrap().starts_with('Z'));
}

#[test]
fn dropping_vm_terminates_its_process_tree_even_with_blocked_stdin() {
    let f = Fixture::new();
    let pidfile = f.dir.join("vm-child.pid");
    let script =
        serde_json::to_string(&format!("echo $$ > '{}'; sleep 30", pidfile.display())).unwrap();
    f.host
        .eval_source(&format!(
            r#"{REPORT}
      globalThis.child = process.spawn('/bin/sh', ['-c', {script}], {{stdin: 'pipe'}});
      child.write('x'.repeat(900000));
    "#
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while !pidfile.exists() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid: u32 = std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let start = Instant::now();
    drop(f);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_process_stopped(pid);
}

#[test]
fn dropping_vm_kills_every_child_at_once_even_when_they_ignore_sigterm() {
    let f = Fixture::new();
    let pids = (0..32)
        .map(|i| f.dir.join(format!("stubborn-{i}.pid")))
        .collect::<Vec<_>>();
    for pidfile in &pids {
        // The shell and its sleep both ignore SIGTERM: only SIGKILL stops them.
        let script = serde_json::to_string(&format!(
            "trap '' TERM; echo $$ > '{}'; sleep 30; sleep 30",
            pidfile.display()
        ))
        .unwrap();
        f.host
            .eval_source(&format!(
                "{REPORT} process.spawn('/bin/sh', ['-c', {script}]);"
            ))
            .unwrap();
    }
    let until = Instant::now() + Duration::from_secs(5);
    while !pids.iter().all(|p| p.exists()) {
        assert!(Instant::now() < until, "children did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(20));
    let pids = pids
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let start = Instant::now();
    drop(f);
    // Serially, a 250 ms grace per child would take 8 seconds.
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "teardown took {:?}",
        start.elapsed()
    );
    for pid in pids {
        assert_process_stopped(pid);
    }
}

#[test]
fn bounded_run_rejects_timeout_and_output_overflow() {
    let f = Fixture::new();
    f.host
        .eval_source(&format!(
            r#"{REPORT}
      process.run('/bin/sh', ['-c', 'sleep 30'], {{timeoutMs: 25}}).catch(e => report(String(e)));
      // `head` is found on the child's PATH, which the host passes on.
      process.run('/bin/sh', ['-c', 'head -c 9437184 /dev/zero']).catch(e => report(String(e)));
    "#
        ))
        .unwrap();
    f.complete();
    let output = f.outputs();
    assert_eq!(output.len(), 2);
    assert!(output.iter().any(|s| s.contains("timed out")));
    assert!(
        output
            .iter()
            .any(|s| s.contains("exceeded 8388608 bytes on one stream")),
        "{output:?}"
    );
}

#[test]
fn reactive_owner_cleanup_cancels_child() {
    let f = Fixture::new();
    let pidfile = f.dir.join("owner-child.pid");
    let script =
        serde_json::to_string(&format!("echo $$ > '{}'; sleep 30", pidfile.display())).unwrap();
    f.host
        .eval_source(&format!(
            r#"{REPORT}
      import {{ signal, effect }} from 'nsh';
      globalThis.enabled = signal(true);
      effect(() => {{
        if (!enabled()) return;
        const owned = process.spawn('/bin/sh', ['-c', {script}]);
        owned.on('exit', r => report(JSON.stringify(r)));
      }});
    "#
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while !pidfile.exists() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    f.host.eval_source("enabled.set(false);").unwrap();
    f.complete();
    assert_eq!(f.outputs().len(), 1);
    let pid: u32 = std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_process_stopped(pid);
}

/// Detached descendants are outside the owned process group. Always reap the
/// test fixture ourselves, including when an assertion fails.
struct DetachedFixture(PathBuf, Option<i32>);
impl Drop for DetachedFixture {
    fn drop(&mut self) {
        let pid = self
            .1
            .or_else(|| std::fs::read_to_string(&self.0).ok()?.trim().parse().ok());
        if let Some(pid) = pid {
            // SAFETY: the PID came from the detached fixture we just launched.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

#[test]
fn detached_inherited_pipes_do_not_block_exit_or_vm_drop() {
    for drop_vm in [false, true] {
        let f = Fixture::new();
        let pidfile = f.dir.join("detached.pid");
        let mut guard = DetachedFixture(pidfile.clone(), None);
        // Explicit stdin redirection prevents the shell from replacing the
        // background child's stdin with /dev/null. The detached sleep keeps all
        // three handles alive and never drains the large stdin write.
        let script = serde_json::to_string(&format!(
            "setsid /bin/sh -c 'echo $$ > \"{}\"; exec sleep 30' <&0 & printf ready; sleep 0.1",
            pidfile.display()
        ))
        .unwrap();
        f.host
            .eval_source(&format!(
                r#"{REPORT}
                globalThis.detached = process.spawn('/bin/sh', ['-c', {script}], {{stdin: 'pipe'}});
                detached.write('x'.repeat(900000));
                detached.on('stdout', text => report(text));
                detached.on('exit', result => report(JSON.stringify(result)));
                "#
            ))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        let pid = loop {
            if let Ok(text) = std::fs::read_to_string(&pidfile)
                && let Ok(pid) = text.trim().parse::<u32>()
            {
                break pid;
            }
            assert!(Instant::now() < until, "detached fixture did not start");
            std::thread::sleep(Duration::from_millis(5));
        };
        guard.1 = Some(pid as i32);
        let start = Instant::now();
        if drop_vm {
            drop(f);
        } else {
            f.complete();
            let outputs = f.outputs();
            assert_eq!(outputs.len(), 2);
            assert_eq!(outputs[0], "ready");
            let exit: serde_json::Value = serde_json::from_str(&outputs[1]).unwrap();
            assert_eq!(exit["code"], 0);
        }
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(guard);
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            if stat.is_err() || stat.unwrap().split(") ").nth(1).unwrap().starts_with('Z') {
                break;
            }
            assert!(
                Instant::now() < until,
                "detached test fixture survived cleanup"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
