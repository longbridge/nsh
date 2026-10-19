//! Standard filesystem entry points share the native capability-checked dispatcher.
use rquickjs::{
    Ctx, Object, Result, Value,
    module::{Declarations, Exports, ModuleDef},
};

pub(crate) struct FsModule;
const EXPORTS: &[&str] = &[
    "readFile",
    "writeFile",
    "mkdir",
    "rename",
    "unlink",
    "exists",
    "access",
    "readdir",
    "stat",
    "lstat",
    "rmdir",
    "constants",
];
impl ModuleDef for FsModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        declarations.declare("promises")?;
        for name in EXPORTS {
            declarations.declare(*name)?;
        }
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let raw = crate::fs::make_standard(ctx.as_raw().as_ptr());
        // SAFETY: raw is a live tagged value returned by make_standard.
        if unsafe { rquickjs::qjs::JS_IsException(raw) } {
            return Err(rquickjs::Error::Exception);
        }
        // SAFETY: make_standard returns an owned value in this live context.
        let value = unsafe { Value::from_raw(ctx.clone(), raw) };
        let fs = value.into_object().ok_or(rquickjs::Error::Unknown)?;
        let promises = Object::new(ctx.clone())?;
        for name in EXPORTS {
            let value: Value = fs.get(*name)?;
            promises.set(*name, value.clone())?;
            exports.export(*name, value)?;
        }
        fs.set("promises", promises.clone())?;
        exports.export("promises", promises)?;
        exports.export("default", fs)?;
        Ok(())
    }
}

/// The supported standard filesystem surface, generated beside its implementation.
pub(crate) const TYPES: &str = r#"
import type { Buffer } from "buffer";
export interface Stats {
 readonly size: number;
 readonly mtimeMs: number;
 isFile(): boolean;
 isDirectory(): boolean;
 isSymbolicLink(): boolean;
}
export interface Dirent {
 readonly name: string;
 isFile(): boolean;
 isDirectory(): boolean;
 isSymbolicLink(): boolean;
}
/** Reads at most 16 MiB through the app's explicit filesystem grants. */
export function readFile(path: string): Promise<Buffer>;
export function readFile(path: string, options: null | { encoding?: null }): Promise<Buffer>;
export function readFile(path: string, encoding: string | { encoding: string }): Promise<string>;
export function writeFile(path: string, data: string | Uint8Array | ArrayBuffer, options?: string | { encoding?: string; flag?: "w" | "wx" }): Promise<void>;
export function mkdir(path: string, options?: { recursive?: boolean }): Promise<void>;
export function rename(from: string, to: string): Promise<void>;
export function unlink(path: string): Promise<void>;
export function exists(path: string): Promise<boolean>;
export function access(path: string, mode?: number): Promise<void>;
export function readdir(path: string, options?: { withFileTypes?: false }): Promise<string[]>;
export function readdir(path: string, options: { withFileTypes: true }): Promise<Dirent[]>;
export function stat(path: string): Promise<Stats>;
export function lstat(path: string): Promise<Stats>;
/** Removes one empty directory. */
export function rmdir(path: string): Promise<void>;
export const constants: { readonly F_OK: 0; readonly R_OK: 4; readonly W_OK: 2 };
export const promises: {
 readFile: typeof readFile; writeFile: typeof writeFile; mkdir: typeof mkdir;
 rename: typeof rename; unlink: typeof unlink; exists: typeof exists;
 access: typeof access; readdir: typeof readdir; stat: typeof stat;
 lstat: typeof lstat; rmdir: typeof rmdir; constants: typeof constants;
};
declare const fs: typeof promises & { promises: typeof promises };
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
    #[test]
    fn standard_aliases_enforce_grants() {
        let (host, dir) = host(false);
        let p = serde_json::to_string(&dir.join("denied")).unwrap();
        host.eval_source(&format!(r#"
 import fs from 'fs'; import {{writeFile}} from 'node:fs'; import promises from 'fs/promises'; import {{readFile}} from 'node:fs/promises';
 globalThis.result='pending'; (async()=>{{let rejected=0;for(const operation of [()=>fs.writeFile({p},'x'),()=>writeFile({p},'x'),()=>fs.promises.writeFile({p},'x'),()=>promises.writeFile({p},'x'),()=>readFile({p})]){{try{{await operation()}}catch(e){{if(e.code==='EACCES')rejected++}}}}globalThis.result=rejected===5?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
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
            import fs from 'fs'; import {{readFile}} from 'node:fs/promises';
            globalThis.result='pending';(async()=>{{let denied=0;for(const op of [()=>readFile({p}),()=>fs.promises.writeFile({p},'changed'),()=>fs.unlink({p}),()=>fs.stat({p})]){{try{{await op()}}catch{{denied++}}}}globalThis.result=denied===4?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
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
    fn binary_roundtrip_and_metadata() {
        let (host, dir) = host(true);
        let p = serde_json::to_string(&dir.join("binary")).unwrap();
        let folder = serde_json::to_string(&dir).unwrap();
        host.eval_source(&format!(r#"
 import fs from 'node:fs'; import {{readFile}} from 'fs/promises';
 globalThis.result='pending';(async()=>{{await fs.writeFile({p},Buffer.from([0,255,128,10]));const bytes=await readFile({p});const info=await fs.stat({p});const entries=await fs.readdir({folder},{{withFileTypes:true}});let exclusive=false;try{{await fs.writeFile({p},'bad',{{flag:'wx'}})}}catch(e){{exclusive=e.code==='EEXIST'}}globalThis.result=Buffer.isBuffer(bytes)&&bytes[1]===255&&bytes.length===4&&info.size===4&&info.isFile()&&entries[0].isFile()&&exclusive?'ok':'bad';}})().catch(e=>globalThis.result=String(e));
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
