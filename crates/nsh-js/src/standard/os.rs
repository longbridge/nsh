//! Small OS metadata adapter. Directory paths grant no filesystem authority.
use rquickjs::{
    Ctx, Exception, Object, Result, Value,
    function::Func,
    module::{Declarations, Exports, ModuleDef},
};
use std::path::PathBuf;

const EXPORTS: &[&str] = &["platform", "arch", "EOL", "homedir"];
pub(crate) struct OsModule;
impl ModuleDef for OsModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        for name in EXPORTS {
            declarations.declare(*name)?;
        }
        Ok(())
    }
    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let os = Object::new(ctx.clone())?;
        os.set(
            "platform",
            Func::from(|| match std::env::consts::OS {
                "macos" => "darwin",
                "windows" => "win32",
                other => other,
            }),
        )?;
        os.set(
            "arch",
            Func::from(|| match std::env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                "x86" => "ia32",
                other => other,
            }),
        )?;
        os.set("EOL", if cfg!(windows) { "\r\n" } else { "\n" })?;
        os.set("homedir", Func::from(home_dir))?;
        exports.export("default", os.clone())?;
        for name in EXPORTS {
            exports.export(*name, os.get::<_, Value>(*name)?)?;
        }
        Ok(())
    }
}
fn home_dir(ctx: Ctx<'_>) -> Result<String> {
    let home = ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
        .ok_or_else(|| {
            Exception::throw_message(
                &ctx,
                "os.homedir(): no absolute native home directory is configured",
            )
        })?;
    Ok(home.to_string_lossy().into_owned())
}

pub(crate) const TYPES: &str = r#"
/** Native OS metadata. Paths returned here grant no filesystem access. */
export function platform(): string;
export function arch(): string;
export const EOL: string;
export function homedir(): string;
declare const os: { platform: typeof platform; arch: typeof arch; EOL: typeof EOL; homedir: typeof homedir };
export default os;
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Module, Runtime};
    #[test]
    fn metadata_has_exact_exports_and_native_values() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let (module, promise) = Module::evaluate_def::<OsModule, _>(ctx.clone(), "os").unwrap();
            promise.finish::<()>().unwrap();
            let object: Object = module.get("default").unwrap();
            assert_eq!(object.keys::<String>().count(), 4);
            assert_eq!(
                object
                    .get::<_, rquickjs::Function>("platform")
                    .unwrap()
                    .call::<_, String>(())
                    .unwrap(),
                match std::env::consts::OS {
                    "macos" => "darwin",
                    "windows" => "win32",
                    other => other,
                }
            );
            assert_eq!(
                object
                    .get::<_, rquickjs::Function>("arch")
                    .unwrap()
                    .call::<_, String>(())
                    .unwrap(),
                match std::env::consts::ARCH {
                    "x86_64" => "x64",
                    "aarch64" => "arm64",
                    "x86" => "ia32",
                    other => other,
                }
            );
            assert_eq!(
                object.get::<_, String>("EOL").unwrap(),
                if cfg!(windows) { "\r\n" } else { "\n" }
            );
        });
    }
    #[test]
    fn home_metadata_does_not_grant_file_access() {
        let host = crate::Host::new().unwrap();
        host.eval_source("import os from 'node:os'; import process from 'process'; globalThis.metadataMatches = os.platform() === process.platform && os.arch() === process.arch; globalThis.homeIsMetadata = typeof os.homedir() === 'string'; globalThis.os = os;").unwrap();
        assert!(
            host.eval_script("metadata", "metadataMatches && homeIsMetadata")
                .unwrap()
        );
        host.eval_source("import fs from 'fs/promises'; globalThis.homeReadDenied = false; fs.readdir(os.homedir()).catch(e => globalThis.homeReadDenied = true);").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !host
            .eval_script("denied-home-read", "homeReadDenied === true")
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            host.run_system();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}
