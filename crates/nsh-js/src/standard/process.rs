//! Standard `process` module over NSH's checked per-VM subprocess runtime.
//!
//! The standard import and the global share one object. The underlying native
//! functions enforce executable/cwd grants and retain the VM's existing lifecycle,
//! queue bounds, streaming and cancellation behavior; this adapter starts no workers.

use rquickjs::{
    Ctx, Error, Exception, Function, Object, Result, Value,
    function::Args,
    module::{Declarations, Exports, ModuleDef},
    object::Accessor,
    prelude::{Func, Opt, Rest},
};

const EXPORTS: &[&str] = &["run", "spawn", "exit", "platform", "arch", "nextTick"];

/// The standard `process` module implementation.
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
    let value = crate::process::standard_module(raw);
    // SAFETY: `module` returns an owned value in this same live context.
    let value = unsafe { Value::from_raw(ctx.clone(), value) };
    let native = value.into_object().ok_or(Error::Exception)?;
    native.set("signalName", Func::from(signal_name))?;
    let wrap: Function = ctx.eval(PROCESS_ADAPTER)?;
    let process: Object = wrap.call((native,))?;
    process.set("exit", Func::from(exit_request))?;
    process.prop("env", Accessor::new_get(granted_env).enumerable())?;
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
    std::env::consts::OS
}
fn arch() -> &'static str {
    std::env::consts::ARCH
}

fn granted_env<'js>(ctx: Ctx<'js>) -> Result<Object<'js>> {
    let env = Object::new(ctx.clone())?;
    let shared = crate::dom::shared(ctx.as_raw().as_ptr());
    for (name, value) in std::env::vars_os() {
        if let (Some(name), Some(value)) = (name.to_str(), value.to_str())
            && shared.system_access.borrow().allows_env(name)
        {
            env.set(name, value)?;
        }
    }
    Ok(env)
}

fn exit_request(ctx: Ctx<'_>, code: Opt<i32>) -> Result<()> {
    let shared = crate::dom::shared(ctx.as_raw().as_ptr());
    if !shared.system_access.borrow().allows_exit() {
        return Err(Exception::throw_type(
            &ctx,
            "process.exit() is not granted; set capabilities.process.exit to true in the manifest",
        ));
    }
    let handler = shared.system_exit_handler.borrow().clone().ok_or_else(|| {
        Exception::throw_message(
            &ctx,
            "process.exit() is granted but this host installed no exit request handler",
        )
    })?;
    handler(code.0.unwrap_or(0));
    Ok(())
}

fn signal_name(signal: Option<i32>) -> Option<String> {
    signal.map(|signal| {
        #[cfg(unix)]
        {
            match signal {
                libc::SIGKILL => "SIGKILL",
                libc::SIGTERM => "SIGTERM",
                libc::SIGINT => "SIGINT",
                libc::SIGHUP => "SIGHUP",
                libc::SIGQUIT => "SIGQUIT",
                libc::SIGABRT => "SIGABRT",
                libc::SIGSEGV => "SIGSEGV",
                libc::SIGPIPE => "SIGPIPE",
                _ => return format!("signal {signal}"),
            }
            .to_owned()
        }
        #[cfg(not(unix))]
        {
            format!("signal {signal}")
        }
    })
}

// Only presentation and stream framing live here; native Rust checks every
// executable, cwd and limit before spawning and owns cleanup independently.
const PROCESS_ADAPTER: &str = r#"
(native => {
  const limit = 8 * 1024 * 1024;
  const encoder = new TextEncoder();
  function spawn(command, args, options) {
    options = options ?? {};
    const stdin = options.stdin ?? 'null';
    if (stdin !== 'pipe' && stdin !== 'null') throw new TypeError('process.spawn: stdin must be "pipe" or "null"');
    if (options.lines !== undefined && typeof options.lines !== 'boolean') throw new TypeError('process.spawn: lines must be a boolean');
    const child = native.spawn(String(command), Array.from(args ?? [], String), {cwd:options.cwd, timeoutMs:options.timeoutMs, stdin:'pipe'});
    const handlers = {stdout:[], stderr:[], exit:[]};
    const pending = {stdout:[], stderr:[]};
    const bytes = {stdout:0, stderr:0};
    let lineError, closed = stdin !== 'pipe';
    if (closed) child.closeStdin();
    const dispatch = (event, payload) => { for (const fn of handlers[event].slice()) fn(payload); };
    const receive = (event, text) => {
      if (!options.lines) { dispatch(event, text); return; }
      const parts = text.split('\n');
      for (let i=0;i<parts.length;i++) {
        pending[event].push(parts[i]); bytes[event] += encoder.encode(parts[i]).length;
        if (bytes[event] > limit) { lineError = 'a line of output exceeded 8 MiB without a newline'; pending[event]=[]; bytes[event]=0; child.kill(); return; }
        if (i < parts.length-1) {
          let line=pending[event].join(''); pending[event]=[]; bytes[event]=0;
          if (line.endsWith('\r')) line=line.slice(0,-1);
          dispatch(event,line);
        }
      }
    };
    child.on('stdout', text => receive('stdout', text));
    child.on('stderr', text => receive('stderr', text));
    child.on('exit', result => {
      for (const event of ['stdout','stderr']) if (bytes[event]) { dispatch(event,pending[event].join('')); pending[event]=[]; bytes[event]=0; }
      const exit = {code:result.code, signal:native.signalName(result.signal) ?? null};
      if (lineError || result.error) exit.error = lineError || result.error;
      dispatch('exit', exit);
    });
    const handle = {
      pid:child.pid,
      on(event, handler) {
        if (!Object.hasOwn(handlers,event)) throw new TypeError('event must be stdout, stderr or exit');
        if (typeof handler !== 'function') throw new TypeError('handler must be a function');
        handlers[event].push(handler); return handle;
      },
      write(text) { if (closed) throw new TypeError('ChildProcess.write needs stdin:"pipe" and an open stdin'); child.write(text); },
      end() { if (!closed) { closed=true; child.closeStdin(); } },
      kill() { child.kill(); },
    };
    return Object.freeze(handle);
  }
  function run(command, args, options) {
    return native.run(command, args, options && typeof options === 'object' ? options : undefined).then(result => ({code:result.code ?? -1, stdout:result.stdout, stderr:result.stderr}), error => { throw new Error(String(error)); });
  }
  return {run,spawn};
})
"#;

/// Module declarations assembled by the standard runtime's declaration writer.
pub(crate) const TYPES: &str = r#"
export interface CommandOutput { code: number; stdout: string; stderr: string; }
export interface RunOptions { cwd?: string; timeoutMs?: number; }
export interface SpawnOptions extends RunOptions { stdin?: "pipe" | "null"; lines?: boolean; }
export interface ExitResult { code: number | null; signal: string | null; error?: string; }
export interface ChildProcess {
  readonly pid: number;
  on(event: "stdout" | "stderr", handler: (text: string) => void): ChildProcess;
  on(event: "exit", handler: (result: ExitResult) => void): ChildProcess;
  write(text: string): void;
  end(): void;
  kill(): void;
}
/** Captures up to 8 MiB per stream, with a default 30-second deadline; child environment is empty. */
export function run(command: string, args?: string[], options?: RunOptions): Promise<CommandOutput>;
/** Streams decoded UTF-8 or complete lines; stdin defaults to null. */
export function spawn(command: string, args?: string[], options?: SpawnOptions): ChildProcess;
/** Requests host-defined exit after capability checks; never exits the host process directly. */
export function exit(code?: number): void;
export const platform: string;
export const arch: string;
export function nextTick<A extends unknown[]>(callback: (...args: A) => void, ...args: A): void;
export interface StandardProcess {
  run: typeof run;
  spawn: typeof spawn;
  exit: typeof exit;
  readonly platform: string;
  readonly arch: string;
  readonly dataDir: string | undefined;
  readonly env: Record<string, string | undefined>;
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
    fn module_shares_global_and_defers_callbacks_with_arguments() {
        let host = Host::new().unwrap();
        host.eval_source(
            r#"
            import process, {nextTick, spawn, run, platform, arch} from 'process';
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
            r#"["/configured-app-root",true,"function","object"]"#
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
            import {spawn, run, nextTick} from 'process';
            import {text} from 'nsh:rt';
            const child = spawn('/bin/sh', ['-c', 'read line; printf "%s" "$line"'], {stdin:'pipe'});
            let output = '';
            child.on('stdout', chunk => output += chunk);
            child.on('exit', result => text(JSON.stringify(['spawn', output, result.code])));
            nextTick(() => { child.write('native-roundtrip\n'); child.end(); });
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
    #[cfg(target_os = "linux")]
    fn checked_host(commands: &[&str]) -> Host {
        let host = Host::new().unwrap();
        host.set_environment(&nsh_core::platform::Environment {
            os: nsh_core::platform::Os::Linux,
            ..Default::default()
        });
        let declared = nsh_core::access::Capabilities::from_json(
            &serde_json::json!({"process":{"execute":commands}}),
        )
        .unwrap();
        host.set_system_access(
            nsh_core::access::Access {
                declared,
                granted: None,
            },
            None,
        );
        host
    }
    #[cfg(target_os = "linux")]
    fn finish(host: &Host) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while host.has_system_tasks() {
            assert!(std::time::Instant::now() < deadline);
            host.run_system();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        host.run_system();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn reference_lines_stdin_chaining_and_empty_child_environment() {
        let host = checked_host(&["/bin/sh", "/usr/bin/env"]);
        host.eval_source(r#"
            import process from 'process'; import {text} from 'nsh:rt';
            const output = [];
            const child = process.spawn('/bin/sh',['-c', "printf 'a\r\nb\nlast'; printf err >&2"],{lines:true});
            const chain = child.on('stdout',line => output.push(['out',line]));
            child.on('stderr',line => output.push(['err',line]));
            let denied=false; try {child.write('x');} catch (_) {denied=true;}
            child.end();
            child.on('exit',result => {
                output.push(['exit',result.code,result.signal]);
                text(JSON.stringify([chain===child,Object.isFrozen(child),denied,output]));
            });
            process.run('/usr/bin/env').then(result => text(JSON.stringify(['env',result.code,result.stdout])));
        "#).unwrap();
        finish(&host);
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let texts = ops
            .into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            texts.iter().any(|text| text == r#"["env",0,""]"#),
            "{texts:?}"
        );
        let line_result: serde_json::Value =
            serde_json::from_str(texts.iter().find(|text| text.starts_with("[true")).unwrap())
                .unwrap();
        assert_eq!(line_result[0], true);
        assert_eq!(line_result[1], true);
        assert_eq!(line_result[2], true);
        let events = line_result[3].as_array().unwrap();
        assert!(events.contains(&serde_json::json!(["out", "a"])));
        assert!(events.contains(&serde_json::json!(["out", "b"])));
        assert!(events.contains(&serde_json::json!(["out", "last"])));
        assert!(events.contains(&serde_json::json!(["err", "err"])));
        assert_eq!(
            events.last().unwrap(),
            &serde_json::json!(["exit", 0, null])
        );
    }
    #[test]
    fn environment_is_a_fresh_granted_snapshot_and_exit_is_a_checked_host_request() {
        use std::{cell::Cell, rc::Rc};
        let host = Host::new().unwrap();
        let out = run_metadata(
            &host,
            r#"
            import process from 'process'; import {text} from 'nsh:rt';
            const a=process.env,b=process.env;
            a.NSH_ONLY_LOCAL='modified';
            let denied=false; try {process.exit();} catch(e) {denied=String(e).includes('capabilities.process.exit');}
            text(JSON.stringify([a!==b,typeof process.env.NSH_ONLY_LOCAL,denied]));
        "#,
        );
        assert_eq!(out, r#"[true,"undefined",true]"#);
        let mut declared = nsh_core::access::Capabilities::default();
        declared.process.exit = true;
        host.set_system_access(
            nsh_core::access::Access {
                declared,
                granted: None,
            },
            None,
        );
        let out = run_metadata(
            &host,
            r#"
            import process from 'process'; import {text} from 'nsh:rt';
            let missing=false;try {process.exit(3);}catch(e){missing=String(e).includes('no exit request handler');}
            text(JSON.stringify(missing));
        "#,
        );
        assert_eq!(out, "true");
        let requested = Rc::new(Cell::new(-1));
        let sink = requested.clone();
        host.set_exit_handler(Rc::new(move |code| sink.set(code)));
        host.eval_source("import {exit} from 'process'; exit(7);")
            .unwrap();
        assert_eq!(requested.get(), 7);
    }
    fn run_metadata(host: &Host, source: &str) -> String {
        host.eval_source(source).unwrap();
        result(host)
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn reference_run_denial_timeout_and_signal_shapes() {
        let host = checked_host(&["/bin/sh", "/no/such/nsh-parity"]);
        host.eval_source(r#"
            import process from 'process';import {text} from 'nsh:rt';
            process.run('/no/such/nsh-parity').catch(e=>text(JSON.stringify(['launch',e instanceof Error])));
            let synchronous=false;try {process.run('not-granted');}catch(e){synchronous=e instanceof TypeError;}
            text(JSON.stringify(['denied',synchronous]));
            process.run('/bin/sh',['-c','sleep 20'],{timeoutMs:0.5}).catch(e=>text(JSON.stringify(['timeout',e instanceof Error,String(e).includes('timed out')])));
            const child=process.spawn('/bin/sh',['-c','sleep 20'],{timeoutMs:20});
            child.on('exit',r=>text(JSON.stringify(['signal',r.code,r.signal,typeof r.error])));
        "#).unwrap();
        finish(&host);
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let texts = ops
            .into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            texts.contains(&r#"["launch",true]"#.to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&r#"["denied",true]"#.to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&r#"["timeout",true,true]"#.to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&r#"["signal",null,"SIGTERM","string"]"#.to_owned()),
            "{texts:?}"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn reference_bounds_are_per_stream_and_long_lines_report_exit_error() {
        let host = checked_host(&["/bin/sh", "/usr/bin/head"]);
        host.eval_source(r#"
            import process from 'process';import {text} from 'nsh:rt';
            process.run('/bin/sh',['-c','/usr/bin/head -c 5242880 /dev/zero; /usr/bin/head -c 5242880 /dev/zero >&2'])
                .then(r=>text(JSON.stringify(['capture',r.code,r.stdout.length,r.stderr.length])));
            const child=process.spawn('/usr/bin/head',['-c','9437184','/dev/zero'],{lines:true});
            child.on('exit',r=>text(JSON.stringify(['line',String(r.error).includes('exceeded 8 MiB')])));
        "#).unwrap();
        finish(&host);
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        let texts = ops
            .into_iter()
            .filter_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            texts.contains(&r#"["capture",0,5242880,5242880]"#.to_owned()),
            "{texts:?}"
        );
        assert!(texts.contains(&r#"["line",true]"#.to_owned()), "{texts:?}");
    }
}
