//! Standard `process` module over NSH's checked per-VM subprocess runtime.
//!
//! Both import spellings and the global share one object. The underlying native
//! functions enforce executable/cwd grants and retain the VM's existing lifecycle,
//! queue bounds, streaming and cancellation behavior; this adapter starts no workers.

use rquickjs::{
    Ctx, Error, Function, Object, Result, Value,
    function::Args,
    module::{Declarations, Exports, ModuleDef},
    object::Accessor,
    prelude::{Func, Rest},
};

const EXPORTS: &[&str] = &["run", "spawn", "platform", "arch", "nextTick"];

/// The `process` and `node:process` module implementation.
pub(crate) struct ProcessModule;

impl ModuleDef for ProcessModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        for name in EXPORTS {
            declarations.declare(*name)?;
        }
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let process: Object = ctx.globals().get("process")?;
        exports.export("default", process.clone())?;
        for name in EXPORTS {
            exports.export(*name, process.get::<_, Value>(*name)?)?;
        }
        Ok(())
    }
}

/// Installs one process facade in this VM, before application code runs.
pub(crate) fn install(ctx: &Ctx<'_>) -> Result<()> {
    let raw = ctx.as_raw().as_ptr();
    let value = crate::process::module(raw);
    // SAFETY: `module` returns an owned value in this same live context.
    let value = unsafe { Value::from_raw(ctx.clone(), value) };
    let process = value.into_object().ok_or(Error::Exception)?;
    process.set("platform", platform())?;
    process.set("arch", arch())?;
    process.set("nextTick", Func::from(next_tick))?;
    process.prop("dataDir", Accessor::new_get(data_dir).enumerable())?;
    ctx.globals().set("process", process)?;
    Ok(())
}

fn data_dir(ctx: Ctx<'_>) -> Option<String> {
    crate::dom::shared(ctx.as_raw().as_ptr())
        .system_data_dir
        .borrow()
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
}

fn next_tick<'js>(ctx: Ctx<'js>, callback: Function<'js>, values: Rest<Value<'js>>) -> Result<()> {
    let mut args = Args::new(ctx, values.len());
    for value in values.0 {
        args.push_arg(value)?;
    }
    callback.defer_arg(args)
}

fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        other => other,
    }
}

/// Module declarations assembled by the standard runtime's declaration writer.
pub(crate) const TYPES: &str = r#"
export const run: import("nsh/process").ProcessModule["run"];
export const spawn: import("nsh/process").ProcessModule["spawn"];
export const platform: string;
export const arch: string;
/** Queues a callback in this VM's microtask queue, after the current JS call. */
export function nextTick<A extends unknown[]>(callback: (...args: A) => void, ...args: A): void;
import type { ProcessModule } from "nsh/process";
export interface StandardProcess extends ProcessModule {
  readonly platform: string;
  readonly arch: string;
  readonly dataDir: string | undefined;
  nextTick: typeof nextTick;
}
declare const process: StandardProcess;
export default process;
"#;

#[cfg(test)]
mod tests {
    use nsh_core::Op;

    use crate::Host;

    fn result(host: &Host) -> String {
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        ops.into_iter()
            .rev()
            .find_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .expect("fixture output")
    }

    #[test]
    fn aliases_share_global_and_defer_callbacks_with_arguments() {
        let host = Host::new().unwrap();
        host.eval_source(
            r#"
            import process, {nextTick, spawn, run, platform, arch} from 'node:process';
            import bare from 'process';
            import {text} from 'nsh:rt';
            const calls = [];
            nextTick((a,b) => { calls.push(a+b); text(JSON.stringify([
                process === bare, bare === globalThis.process,
                spawn === process.spawn, run === process.run,
                platform === process.platform, arch === process.arch,
                typeof process.dataDir, calls
            ])); }, 2, 3);
            calls.push('sync');
        "#,
        )
        .unwrap();
        assert_eq!(
            result(&host),
            r#"[true,true,true,true,true,true,"undefined",["sync",5]]"#
        );
    }

    #[test]
    fn data_directory_getter_tracks_host_configuration_without_host_authority() {
        let host = Host::new().unwrap();
        let path = std::path::PathBuf::from("/configured-app-root");
        host.set_system_access(nsh_core::access::Access::default(), Some(path));
        host.eval_source(
            r#"
            import process from 'process'; import {text} from 'nsh:rt';
            let denied = false;
            try { process.spawn('not-granted'); } catch (_) { denied = true; }
            text(JSON.stringify([process.dataDir, denied,
                typeof process.exit, typeof process.env]));
        "#,
        )
        .unwrap();
        assert_eq!(
            result(&host),
            r#"["/configured-app-root",true,"undefined","undefined"]"#
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn standard_spawn_and_run_use_the_checked_worker_with_llrt_jobs() {
        use std::time::{Duration, Instant};

        use nsh_core::{
            access::{Access, Capabilities},
            platform::{Environment, Os},
        };
        let root = std::env::var_os("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                    .join(".cache/nsh-benchs-test-tmp")
            });
        assert!(!root.starts_with("/tmp"));
        std::fs::create_dir_all(&root).unwrap();
        let host = Host::new().unwrap();
        host.set_environment(&Environment {
            os: Os::Linux,
            ..Default::default()
        });
        let declared = Capabilities::from_json(&serde_json::json!({
            "process": {"execute": ["/bin/sh"]}, "fs": {"read": ["${dataDir}"]}
        }))
        .unwrap();
        host.set_system_access(
            Access {
                declared,
                granted: None,
            },
            Some(root),
        );
        host.eval_source(
            r#"
            import {spawn, run, nextTick} from 'node:process';
            import {text} from 'nsh:rt';
            const child = spawn('/bin/sh', ['-c', 'read line; printf "%s" "$line"']);
            let output = '';
            child.on('stdout', chunk => output += chunk);
            child.on('exit', result => text(JSON.stringify(['spawn', output, result.code])));
            nextTick(() => { child.write('native-roundtrip\n'); child.closeStdin(); });
            run('/bin/sh', ['-c', 'printf native-run']).then(result =>
                text(JSON.stringify(['run', result.stdout, result.code])));
        "#,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while host.has_system_tasks() {
            assert!(Instant::now() < deadline);
            host.run_system();
            std::thread::sleep(Duration::from_millis(5));
        }
        host.run_system();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let outputs = ops
            .into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            outputs
                .iter()
                .any(|s| s == r#"["spawn","native-roundtrip",0]"#)
        );
        assert!(outputs.iter().any(|s| s == r#"["run","native-run",0]"#));
    }
}
