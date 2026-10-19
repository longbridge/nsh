//! `fs/promises`'s module and declarations; the operations are in `crate::fs`.
use rquickjs::{
    Ctx, Result, Value,
    module::{Declarations, Exports, ModuleDef},
};

#[non_exhaustive]
#[derive(Default)]
pub(crate) struct FsModule;
const EXPORTS: &[&str] = &[
    "readFile",
    "writeFile",
    "readdir",
    "exists",
    "unlink",
    "rmdir",
    "mkdir",
    "stat",
    "lstat",
    "rename",
    "readlink",
];
impl ModuleDef for FsModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        for name in EXPORTS {
            declarations.declare(*name)?;
        }
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let raw = crate::fs::module(ctx.as_raw().as_ptr());
        // SAFETY: raw is a live tagged value returned by module.
        if unsafe { rquickjs::qjs::JS_IsException(raw) } {
            return Err(rquickjs::Error::Exception);
        }
        // SAFETY: module returns an owned value in this live context.
        let value = unsafe { Value::from_raw(ctx.clone(), raw) };
        let fs = value.into_object().ok_or(rquickjs::Error::Unknown)?;
        for name in EXPORTS {
            exports.export(*name, fs.get::<_, Value>(*name)?)?;
        }
        exports.export("default", fs)?;
        Ok(())
    }
}

/// The supported standard filesystem surface, generated beside its implementation.
pub(crate) const TYPES: &str = r#"
export interface Stats {
 size: number; mode: number;
 mtimeMs: number; atimeMs: number; birthtimeMs: number;
 mtime: Date; atime: Date; birthtime: Date;
 isFile(): boolean; isDirectory(): boolean; isSymbolicLink(): boolean;
}
export interface Dirent { name: string; isDirectory(): boolean; }
export interface MakeDirectoryOptions { recursive?: boolean; }
/** Reads at most 64 MiB. Omitted encoding returns Uint8Array. A runtime holds at most 64 MiB of file data in flight. */
export function readFile(path: string): Promise<Uint8Array>;
export function readFile(path: string, encoding: "utf8" | { encoding: "utf8" }): Promise<string>;
/** Writes UTF-8 or Uint8Array bytes, replacing an existing regular file; at most 8 MiB. */
export function writeFile(path: string, data: string | Uint8Array): Promise<void>;
export function mkdir(path: string, options?: MakeDirectoryOptions): Promise<void>;
export function rename(from: string, to: string): Promise<void>;
/** Removes a file or a symlink itself, never its target. */
export function unlink(path: string): Promise<void>;
/** Whether the entry exists; a symlink exists even when its target is missing. */
export function exists(path: string): Promise<boolean>;
export function readdir(path: string): Promise<string[]>;
export function readdir(path: string, options: { withFileTypes: true }): Promise<Dirent[]>;
export function stat(path: string): Promise<Stats>;
export function lstat(path: string): Promise<Stats>;
export function readlink(path: string): Promise<string>;
/** Removes one empty directory. */
export function rmdir(path: string): Promise<void>;
declare const fs: { readFile: typeof readFile; writeFile: typeof writeFile;
 readdir: typeof readdir; exists: typeof exists; unlink: typeof unlink;
 rmdir: typeof rmdir; mkdir: typeof mkdir; stat: typeof stat;
 lstat: typeof lstat; rename: typeof rename; readlink: typeof readlink; };
export default fs;
"#;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };

    use crate::Host;
    fn host(allowed: bool) -> (Host, PathBuf) {
        let root = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/nsh-benchs-test-tmp")
            });
        assert!(!root.starts_with("/tmp"));
        let dir = root.join(format!(
            "standard-fs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let host = Host::new().unwrap();
        host.set_environment(&nsh_core::platform::Environment {
            os: nsh_core::platform::Os::Linux,
            ..Default::default()
        });
        let caps = nsh_core::access::Capabilities::from_json(&if allowed {
            serde_json::json!({"fs":{"read":["${dataDir}"],"write":["${dataDir}"]}})
        } else {
            serde_json::json!({})
        })
        .unwrap();
        host.set_system_access(
            nsh_core::access::Access {
                declared: caps,
                granted: None,
            },
            Some(dir.clone()),
        );
        host.set_system_wake(Arc::new(|| {}));
        (host, dir)
    }
    fn finish(host: &Host) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.has_system_tasks() {
            assert!(Instant::now() < deadline, "filesystem work timed out");
            host.run_system();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    // Exact script from gpui-shell's stat_rename_and_readlink_settle_through_promises.
    // The optional environment variable verifies this copy against the reference checkout.
    const REFERENCE_METADATA_PROBE: &str = r#"
import { div, View } from "gpui-kit";
import { v_flex } from "gpui-base";
import * as fs from "fs/promises";

export default class Probe extends View {
  init(_props, cx) {
    this.state = "pending";
    cx.spawn(async (cx) => {
      try {
        await fs.writeFile("draft.txt", "hello");
        const file = await fs.stat("draft.txt");
        const root = await fs.stat(".");
        const fresh = Date.now() - file.mtimeMs < 60000 && file.mtime instanceof Date;
        await fs.rename("draft.txt", "final.txt");
        const moved = !(await fs.exists("draft.txt")) && await fs.readFile("final.txt", "utf8");
        const link = await fs.lstat("link.txt");
        const target = await fs.readlink("link.txt");
        const followed = await fs.stat("link.txt");
        this.state = [
          `${file.isFile()}:${file.isDirectory()}:${file.size}:${fresh}`,
          `${root.isDirectory()}`,
          moved,
          `${link.isSymbolicLink()}:${target}:${followed.isFile()}:${followed.isSymbolicLink()}`,
        ].join("|");
      } catch (error) {
        this.state = `failed: ${error.message}`;
      }
      cx.notify();
    });
  }
  render() { return v_flex().child(this.state); }
}
"#;
    #[test]
    fn reference_metadata_script_matches_gpui_shell_contract() {
        use std::os::unix::fs::symlink;
        if let Some(reference) = std::env::var_os("NSH_GPUI_SHELL_REFERENCE") {
            let source = std::fs::read_to_string(
                PathBuf::from(reference).join("crates/shell/src/tests/fs.rs"),
            )
            .unwrap();
            let marker = "const METADATA_PROBE: &str = r#\"";
            let body = source
                .split_once(marker)
                .unwrap()
                .1
                .split_once("\"#;")
                .unwrap()
                .0;
            assert_eq!(
                REFERENCE_METADATA_PROBE, body,
                "refresh the exact reference probe when its contract changes"
            );
            assert!(source.contains("true:false:5:true|true|hello|true:target.txt:true:false"));
        }
        let (host, dir) = host(true);
        std::fs::write(dir.join("target.txt"), "linked").unwrap();
        symlink("target.txt", dir.join("link.txt")).unwrap();
        let source = REFERENCE_METADATA_PROBE
            .replace("import { div, View } from \"gpui-kit\";", "class View {}")
            .replace(
                "import { v_flex } from \"gpui-base\";",
                "const v_flex=()=>({child:value=>value});",
            )
            .replace("export default class Probe", "class Probe");
        host.eval_source(&(source+"\nconst probe=new Probe();probe.init({}, {spawn: task=>task({notify(){globalThis.result=probe.state}})}); ")).unwrap();
        finish(&host);
        assert!(
            host.eval_script(
                "reference contract",
                "if(result !== 'true:false:5:true|true|hello|true:target.txt:true:false')throw Error(result);true"
            )
            .unwrap()
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    const REFERENCE_IO_PROBE: &str = r#"
import { div, View } from "gpui-kit";
import { v_flex } from "gpui-base";
import * as fs from "fs/promises";

export default class Probe extends View {
  init(_props, cx) {
    this.state = "pending";

    cx.spawn(async (cx) => {
      try {
        await fs.writeFile("notes.txt", "hello");
        await fs.writeFile("bytes.bin", new Uint8Array([0, 255, 42]));
        const back = await fs.readFile("notes.txt", "utf8");
        const bytes = await fs.readFile("bytes.bin");
        const names = await fs.readdir(".");
        const entries = await fs.readdir(".", { withFileTypes: true });
        const entryShape = entries.every((entry) =>
          typeof entry.name === "string" && typeof entry.isDirectory() === "boolean"
        );
        const there = await fs.exists("notes.txt");

        await fs.mkdir("nested/deeper", { recursive: true });
        const nested = await fs.exists("nested/deeper");

        await fs.unlink("notes.txt");
        const gone = !(await fs.exists("notes.txt"));

        this.state = `${back}|${bytes instanceof Uint8Array}:${[...bytes].join(",")}|${names.join(",")}|${entryShape}|${there}|${nested}|${gone}`;
      } catch (error) {
        this.state = `failed: ${error.message}`;
      }
      cx.notify();
    });
  }

  render() {
    return v_flex().child(this.state);
  }
}
"#;
    #[test]
    fn reference_io_script_matches_gpui_shell_contract() {
        if let Some(reference) = std::env::var_os("NSH_GPUI_SHELL_REFERENCE") {
            let source = std::fs::read_to_string(
                PathBuf::from(reference).join("crates/shell/src/tests/fs.rs"),
            )
            .unwrap();
            let body = source
                .split_once("const PROBE: &str = r#\"")
                .unwrap()
                .1
                .split_once("\"#;")
                .unwrap()
                .0;
            assert_eq!(REFERENCE_IO_PROBE, body);
            assert!(source.contains("hello|true:0,255,42|bytes.bin,notes.txt|true|true|true|true"));
        }
        let (host, dir) = host(true);
        let source = REFERENCE_IO_PROBE
            .replace("import { div, View } from \"gpui-kit\";", "class View {}")
            .replace(
                "import { v_flex } from \"gpui-base\";",
                "const v_flex=()=>({child:value=>value});",
            )
            .replace("export default class Probe", "class Probe");
        host.eval_source(&(source+"\nconst probe=new Probe();probe.init({}, {spawn: task=>task({notify(){globalThis.result=probe.state}})}); ")).unwrap();
        finish(&host);
        assert!(
            host.eval_script(
                "reference io contract",
                "result === 'hello|true:0,255,42|bytes.bin,notes.txt|true|true|true|true'"
            )
            .unwrap()
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn standard_surface_options_errors_and_link_operations_match_reference() {
        use std::os::unix::fs::symlink;
        let (host, dir) = host(true);
        let outside = dir.with_extension("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret"), "safe").unwrap();
        std::fs::write(dir.join("invalid"), [0xff]).unwrap();
        symlink(outside.join("secret"), dir.join("link")).unwrap();
        symlink(outside.join("secret"), dir.join("absolute")).unwrap();
        symlink(dir.join("missing"), dir.join("dangling")).unwrap();
        let absolute = serde_json::to_string(&dir.join("absolute")).unwrap();
        host.eval_source(&r#"
        import fs from 'fs/promises';import * as namespace from 'fs/promises';
        const absoluteLink=ABSOLUTE;
        globalThis.result='pending';(async()=>{
          const expected=['default','exists','lstat','mkdir','readFile','readdir','readlink','rename','rmdir','stat','unlink','writeFile'];
          if(JSON.stringify(Object.keys(namespace).sort())!==JSON.stringify(expected.sort()))throw Error('exports differ');
          let sync=0;
          for(const op of [()=>fs.readFile('invalid','base64'),()=>fs.readFile('invalid',{}),()=>fs.readFile('invalid',{encoding:'utf8',other:true}),()=>fs.mkdir('never',{recursive:'yes'}),()=>fs.mkdir('never',{typo:true}),()=>fs.readdir('.',{withFileTypes:1}),()=>fs.readdir('.',{recursive:true}),()=>fs.exists('/outside-native-root'),()=>fs.writeFile('never',new ArrayBuffer(4)),()=>fs.writeFile('never',new Uint16Array(2))]){try{op()}catch(e){if(e instanceof TypeError)sync++}}
          if(sync!==10)throw Error('sync checks '+sync);
          let getterCalls=0;await fs.readdir('.',{get withFileTypes(){getterCalls++;return true}});if(getterCalls!==1)throw Error('option getter called '+getterCalls+' times');
          let invalid=false;try{await fs.readFile('invalid','utf8')}catch(e){invalid=e.constructor===Error&&e.message.includes('invalid')&&e.message.includes('UTF-8')}
          if(!invalid)throw Error('UTF-8 error differs');
          const link=await fs.lstat('link');const target=await fs.readlink('link');
          if(!link.isSymbolicLink()||!target.endsWith('/secret'))throw Error('link was followed');
          let denied=false;try{await fs.readFile('link')}catch(e){denied=e.constructor===Error&&!e.message.includes('capabilities.fs')}
          if(!denied)throw Error('outside read allowed');if(!(await fs.exists('link')))throw Error('link missed');
          if(!(await fs.exists('dangling'))||!(await fs.lstat('dangling')).isSymbolicLink())throw Error('dangling link missed');
          await fs.unlink('dangling');if(await fs.exists('dangling'))throw Error('dangling link kept');
          await fs.unlink(absoluteLink);if(await fs.exists('absolute'))throw Error('absolute link kept');
          await fs.rename('link','moved');if(!(await fs.lstat('moved')).isSymbolicLink())throw Error('rename followed');
          await fs.unlink('moved');await fs.mkdir('empty');if(await fs.rmdir('empty')!==undefined)throw Error('rmdir return');
          await fs.mkdir('nonempty');await fs.writeFile('nonempty/file','keep');let rejected=false;try{await fs.rmdir('nonempty')}catch(e){rejected=e.constructor===Error}
          if(!rejected||!await fs.exists('nonempty/file'))throw Error('recursive removal');
          globalThis.result='ok';
        })().catch(e=>globalThis.result=String(e));
      "#.replace("ABSOLUTE", &absolute)).unwrap();
        finish(&host);
        assert!(
            host.eval_script(
                "parity options and links",
                "if(result!=='ok')throw Error(result);true"
            )
            .unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("secret")).unwrap(),
            "safe"
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn standard_read_and_write_limits_match_reference() {
        let (host, dir) = host(true);
        let big = std::fs::File::create(dir.join("over-read-limit")).unwrap();
        big.set_len(64 * 1024 * 1024 + 1).unwrap();
        let allowed = std::fs::File::create(dir.join("above-native-text-limit")).unwrap();
        allowed.set_len(16 * 1024 * 1024 + 1).unwrap();
        host.eval_source(r#"import fs from 'fs/promises';globalThis.result='pending';(async()=>{let read=false;try{await fs.readFile('over-read-limit')}catch(e){read=e.constructor===Error&&e.message.includes('67108864')}let write=false;try{fs.writeFile('not-created','x'.repeat(8*1024*1024+1))}catch(e){write=e instanceof RangeError}const bytes=await fs.readFile('above-native-text-limit');globalThis.result=read&&write&&Object.getPrototypeOf(bytes)===Uint8Array.prototype&&bytes.length===16*1024*1024+1?'ok':'bad'})().catch(e=>globalThis.result=String(e));"#).unwrap();
        finish(&host);
        assert!(
            host.eval_script("reference limits", "result==='ok'")
                .unwrap()
        );
        assert!(!dir.join("not-created").exists());
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn explicit_absolute_roots_work_without_an_app_data_service() {
        let (configured, dir) = host(true);
        drop(configured);
        let host = Host::new().unwrap();
        host.eval_source("import fs from 'fs/promises';globalThis.denied=false;try{fs.readdir('/home')}catch(e){globalThis.denied=e instanceof TypeError&&e.message.includes('capabilities.fs')}").unwrap();
        assert!(host.eval_script("no grants", "denied").unwrap());
        host.set_environment(&nsh_core::platform::Environment {
            os: nsh_core::platform::Os::Linux,
            ..Default::default()
        });
        let caps = nsh_core::access::Capabilities::from_json(
            &serde_json::json!({"fs":{"read":[dir],"write":[dir]}}),
        )
        .unwrap();
        host.set_system_access(
            nsh_core::access::Access {
                declared: caps,
                granted: None,
            },
            None,
        );
        let file = serde_json::to_string(&dir.join("plain")).unwrap();
        host.eval_source(&format!("import fs from 'fs/promises';globalThis.result='pending';(async()=>{{await fs.writeFile({file},'plain');globalThis.result=await fs.readFile({file},'utf8')}})().catch(e=>globalThis.result=String(e));")).unwrap();
        finish(&host);
        assert!(
            host.eval_script("plain filesystem grant", "result==='plain'")
                .unwrap()
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn trusted_plugin_home_and_relative_roots_are_resolved_by_host_anchors() {
        let (host, dir) = host(true);
        let plugin = dir.join("plugin");
        let home = dir.join("home");
        for path in [
            plugin.join("resources"),
            plugin.join("relative"),
            home.join("exports"),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut anchors = nsh_core::access::TrustedPaths::default();
        anchors.plugin_dir = Some(plugin.clone());
        anchors.home_dir = Some(home.clone());
        host.set_system_paths(anchors);
        let caps=nsh_core::access::Capabilities::from_json(&serde_json::json!({"fs":{"read":["${pluginDir}/resources","relative","${homeDir}/exports"],"write":["${pluginDir}/resources","relative","${homeDir}/exports"]}})).unwrap();
        host.set_system_access(
            nsh_core::access::Access {
                declared: caps,
                granted: None,
            },
            Some(dir.clone()),
        );
        let relative = serde_json::to_string(&plugin.join("relative/second.txt")).unwrap();
        host.eval_source(&format!(r#"import fs from 'fs/promises';globalThis.result='pending';(async()=>{{await fs.writeFile('first.txt','first');await fs.writeFile({relative},'second');await fs.writeFile('${{homeDir}}/exports/home.txt','home');await fs.writeFile('${{pluginDir}}/resources/native.txt','native');const back=await fs.readFile('${{pluginDir}}/resources/native.txt','utf8');globalThis.result=(await fs.readFile('first.txt','utf8'))==='first'&&(await fs.readFile({relative},'utf8'))==='second'&&back==='native'?'ok':'bad';}})().catch(e=>globalThis.result=String(e));"#)).unwrap();
        finish(&host);
        assert!(
            host.eval_script("trusted anchors", "result==='ok'")
                .unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(home.join("exports/home.txt")).unwrap(),
            "home"
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn standard_named_and_default_exports_enforce_grants() {
        let (host, dir) = host(false);
        let p = serde_json::to_string(&dir.join("denied")).unwrap();
        host.eval_source(&format!(r#"
 import fs, {{writeFile,readFile,exists}} from 'fs/promises';
 globalThis.result='pending'; (async()=>{{let rejected=0;for(const operation of [()=>fs.writeFile({p},'x'),()=>writeFile({p},'x'),()=>fs.readFile({p}),()=>readFile({p}),()=>exists({p})]){{try{{await operation()}}catch(e){{if(e instanceof TypeError&&e.message.includes('capabilities.fs'))rejected++}}}}globalThis.result=rejected===5?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
 "#)).unwrap();
        finish(&host);
        assert!(host.eval_script("assert", "result === 'ok'").unwrap());
        assert!(!dir.join("denied").exists());
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn symlink_escape_is_denied_through_standard_fs() {
        use std::os::unix::fs::symlink;
        let (host, dir) = host(true);
        let outside = dir.with_extension("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret"), "untouched").unwrap();
        symlink(&outside, dir.join("escape")).unwrap();
        let p = serde_json::to_string(&dir.join("escape/secret")).unwrap();
        host.eval_source(&format!(r#"
            import fs from 'fs/promises'; import {{readFile}} from 'fs/promises';
            globalThis.result='pending';(async()=>{{let denied=0;for(const op of [()=>readFile({p}),()=>fs.writeFile({p},'changed'),()=>fs.unlink({p}),()=>fs.stat({p})]){{try{{await op()}}catch{{denied++}}}}globalThis.result=denied===4?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
        "#)).unwrap();
        finish(&host);
        assert!(host.eval_script("assert", "result === 'ok'").unwrap());
        assert_eq!(
            std::fs::read_to_string(outside.join("secret")).unwrap(),
            "untouched"
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn data_in_flight_is_bounded_per_runtime() {
        let (host, dir) = host(true);
        for name in ["first", "second"] {
            let file = std::fs::File::create(dir.join(name)).unwrap();
            file.set_len(40 * 1024 * 1024).unwrap();
        }
        // Writes hold their data from the call; nothing settles within one turn.
        host.eval_source(r#"import fs from 'fs/promises';globalThis.writes=0;const data='x'.repeat(8*1024*1024);try{for(let i=0;i<9;i++){fs.writeFile('w'+i,data);writes++}}catch(e){globalThis.refused=e instanceof RangeError&&e.message.includes('in flight')}"#).unwrap();
        assert!(
            host.eval_script("writes in flight", "writes===8&&refused")
                .unwrap()
        );
        finish(&host);
        // Both reads finish before JS takes either result: the second does not fit.
        host.eval_source(r#"import fs from 'fs/promises';globalThis.reads=[];for(const name of ['first','second'])fs.readFile(name).then(b=>reads.push(b.length),e=>reads.push(e.message));const until=Date.now()+1500;while(Date.now()<until){}"#).unwrap();
        finish(&host);
        assert!(
            host.eval_script(
                "reads in flight",
                "reads.length===2&&reads.includes(40*1024*1024)&&reads.some(r=>String(r).includes('in flight'))"
            )
            .unwrap()
        );
        // Settled data returns to the budget.
        host.eval_source(r#"import fs from 'fs/promises';globalThis.again='pending';fs.readFile('second').then(b=>globalThis.again=b.length,e=>globalThis.again=e.message)"#).unwrap();
        finish(&host);
        assert!(
            host.eval_script("budget released", "again===40*1024*1024")
                .unwrap()
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn binary_roundtrip_and_metadata() {
        let (host, dir) = host(true);
        let p = serde_json::to_string(&dir.join("binary")).unwrap();
        let folder = serde_json::to_string(&dir).unwrap();
        host.eval_source(&format!(r#"
 import fs from 'fs/promises'; import {{readFile}} from 'fs/promises';
 globalThis.result='pending';(async()=>{{await fs.writeFile({p},Buffer.from([0,255,128,10]));const bytes=await readFile({p});const info=await fs.stat({p});const entries=await fs.readdir({folder},{{withFileTypes:true}});globalThis.result=Object.getPrototypeOf(bytes)===Uint8Array.prototype&&bytes[1]===255&&bytes.length===4&&info.size===4&&info.isFile()&&!entries[0].isDirectory()&&info.mtime instanceof Date&&typeof info.mode==='number'?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
 "#)).unwrap();
        finish(&host);
        assert!(host.eval_script("assert", "result === 'ok'").unwrap());
        assert_eq!(
            std::fs::read(dir.join("binary")).unwrap(),
            [0, 255, 128, 10]
        );
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
