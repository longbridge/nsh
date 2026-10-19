//! Native handle ownership regressions (unit scope inspects the VM registry).
#![cfg(target_os = "linux")]

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use nsh_core::{
    access::{Access, Capabilities},
    platform::{Environment, Os},
};

use crate::Host;

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn configured() -> (Host, PathBuf) {
    let root = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".cache/nsh-benchs-test-tmp")
        });
    assert!(!root.starts_with("/tmp"));
    let dir = root.join(format!(
        "native-process-handles-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let host = Host::new().unwrap();
    host.set_environment(&Environment {
        os: Os::Linux,
        ..Default::default()
    });
    let caps = Capabilities::from_json(&serde_json::json!({
        "process": {"execute": ["/bin/sh"]}, "fs": {"read": ["${dataDir}"]}
    }))
    .unwrap();
    host.set_system_access(
        Access {
            declared: caps,
            granted: None,
        },
        Some(dir.clone()),
    );
    (host, dir)
}
fn finish(host: &Host) {
    let until = Instant::now() + Duration::from_secs(3);
    while host.has_system_tasks() {
        assert!(Instant::now() < until);
        host.run_system();
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn sequential_spawn_and_gc_leave_no_retired_registry_entries() {
    let (host, dir) = configured();
    host.eval_source("import process from 'process'; globalThis.Process = process;")
        .unwrap();
    for _ in 0..100 {
        // Even a listener capturing its child must stop rooting the child at exit.
        host.eval_source("{ const child = Process.spawn('/bin/sh', ['-c','exit 0']); child.on('exit', () => child.pid); }").unwrap();
        let handle = host.with_raw(|ctx| {
            super::processes(ctx)
                .entries
                .borrow()
                .values()
                .next()
                .unwrap()
                .handle
                .clone()
        });
        finish(&host);
        host.with_raw(|ctx| {
            assert!(super::processes(ctx).entries.borrow().is_empty());
            // SAFETY: collection on this live VM's owning thread.
            unsafe {
                rquickjs::qjs::JS_RunGC(rquickjs::qjs::JS_GetRuntime(ctx));
            }
        });
        assert!(
            handle.upgrade().is_none(),
            "native child handle survived collection"
        );
    }
    drop(host);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retained_child_delivers_late_exit_after_registry_release_and_gc() {
    let (host, dir) = configured();
    host.eval_source("import process from 'process'; globalThis.child = process.spawn('/bin/sh', ['-c','exit 17']);").unwrap();
    finish(&host);
    host.with_raw(|ctx| {
        assert!(super::processes(ctx).entries.borrow().is_empty());
        // SAFETY: collection on the live VM's owning thread.
        unsafe {
            rquickjs::qjs::JS_RunGC(rquickjs::qjs::JS_GetRuntime(ctx));
        }
    });
    host.eval_source(
        "import {text} from 'nsh:rt'; child.on('exit', r => text(String(r.code))); child.kill();",
    )
    .unwrap();
    let mut ops = Vec::new();
    host.take_ops(&mut ops);
    assert!(
        ops.iter()
            .any(|op| matches!(op, nsh_core::Op::CreateText {text, ..} if text == "17"))
    );
    drop(host);
    // Another concurrently running test may share this harmless fixture directory.
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn process_cwd_uses_host_plugin_and_home_anchors_with_escape_checks() {
    let (host, dir) = configured();
    let plugin = dir.join("plugin");
    let home = dir.join("home");
    let outside = dir.join("outside");
    for path in [&plugin, &home, &outside] {
        std::fs::create_dir_all(path).unwrap();
    }
    std::os::unix::fs::symlink(&outside, plugin.join("escape")).unwrap();
    let mut paths = nsh_core::access::TrustedPaths::default();
    paths.plugin_dir = Some(plugin.clone());
    paths.home_dir = Some(home.clone());
    host.set_system_paths(paths);
    let declared = Capabilities::from_json(&serde_json::json!({
        "process": {"execute": ["/bin/sh"]},
        "fs": {"write": ["${pluginDir}", "${homeDir}"]}
    }))
    .unwrap();
    host.set_system_access(
        Access {
            declared,
            granted: None,
        },
        Some(dir.clone()),
    );
    host.eval_source(
        r#"
        import process from 'process';
        import {text} from 'nsh:rt';
        process.run('/bin/sh',['-c','pwd'],{cwd:'${pluginDir}'})
            .then(r=>text(JSON.stringify(['plugin',r.stdout.trim()])));
        process.run('/bin/sh',['-c','pwd'],{cwd:'${homeDir}'})
            .then(r=>text(JSON.stringify(['home',r.stdout.trim()])));
        let denied=false;
        try {process.spawn('/bin/sh',[],{cwd:'${pluginDir}/escape'});} catch(e) {
            denied=String(e).includes('not granted');
        }
        text(JSON.stringify(['escape',denied]));
    "#,
    )
    .unwrap();
    finish(&host);
    let mut ops = Vec::new();
    host.take_ops(&mut ops);
    let output = ops
        .into_iter()
        .filter_map(|op| match op {
            nsh_core::Op::CreateText { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        output.contains(&serde_json::json!(["plugin", plugin.to_string_lossy()]).to_string()),
        "{output:?}"
    );
    assert!(
        output.contains(&serde_json::json!(["home", home.to_string_lossy()]).to_string()),
        "{output:?}"
    );
    assert!(
        output.contains(&r#"["escape",true]"#.to_owned()),
        "{output:?}"
    );
    drop(host);
    std::fs::remove_dir_all(dir).unwrap();
}
