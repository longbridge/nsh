//! Standard console imports reuse NSH's per-VM logger and its formatting.
//!
//! The facade exports the existing global object without adding stream handles,
//! environment access or another logger. Its surface matches the five methods
//! installed by `crate::console`; no Node Console constructor is advertised.

use rquickjs::{
    Ctx, Object, Result,
    module::{Declarations, Exports, ModuleDef},
};

/// The shared implementation of `console` and `node:console`.
pub(crate) struct ConsoleModule;

impl ModuleDef for ConsoleModule {
    fn declare(declarations: &Declarations) -> Result<()> {
        declarations.declare("default")?;
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
        let console: Object = ctx.globals().get("console")?;
        exports.export("default", console)?;
        Ok(())
    }
}

/// Generated declarations of the exact console facade served above.
pub(crate) const TYPES: &str = r#"
/** Per-VM logging through the host's logger, using NSH console formatting. */
export interface NshConsole {
  log(...values: unknown[]): void;
  info(...values: unknown[]): void;
  debug(...values: unknown[]): void;
  warn(...values: unknown[]): void;
  error(...values: unknown[]): void;
}
declare const console: NshConsole;
export default console;
"#;

#[cfg(test)]
mod tests {
    use nsh_core::Op;

    use crate::Host;

    fn run(host: &Host, source: &str) -> String {
        host.eval_source(source).unwrap();
        let mut ops = Vec::new();
        host.take_ops(&mut ops);
        ops.into_iter()
            .rev()
            .find_map(|op| match op {
                Op::CreateText { text, .. } => Some(text),
                _ => None,
            })
            .expect("console fixture output")
    }

    #[test]
    fn aliases_export_the_existing_logger_object_and_only_the_supported_surface() {
        let host = Host::new().unwrap();
        let out = run(
            &host,
            r#"
            import bare from 'console';
            import node, * as namespace from 'node:console';
            import {text} from 'nsh:rt';
            for (const name of ['log','info','debug','warn','error']) {
                bare[name]('console module fixture', {ok:true});
            }
            text(JSON.stringify([bare === node, node === globalThis.console,
                Object.keys(namespace).sort(), Object.keys(node).sort(),
                ['log','info','debug','warn','error'].every(name =>
                    typeof node[name] === 'function' && node[name] === globalThis.console[name]),
                typeof node.Console]));
        "#,
        );
        assert_eq!(
            out,
            r#"[true,true,["default"],["debug","error","info","log","warn"],true,"undefined"]"#
        );
    }

    #[test]
    fn console_objects_are_isolated_between_vms() {
        let first = Host::new().unwrap();
        let second = Host::new().unwrap();
        first
            .eval_source("import console from 'console'; console.localMarker = 'first';")
            .unwrap();
        let out = run(
            &second,
            r#"
            import console from 'node:console'; import {text} from 'nsh:rt';
            text(JSON.stringify([typeof console.localMarker, console === globalThis.console]));
        "#,
        );
        assert_eq!(out, r#"["undefined",true]"#);
    }
}
