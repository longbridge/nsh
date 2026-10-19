//! Errors are reported with app source locations, via the source map.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use nsh_core::{ApproxTextEngine, MountBatch, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};

struct Sink {
    errors: Arc<Mutex<mpsc::Sender<String>>>,
}

impl MountSink for Sink {
    fn submit(&self, _batch: MountBatch) {}

    fn report_error(&self, message: &str) {
        let _ = self.errors.lock().unwrap().send(message.to_owned());
    }
}

/// Bundles an app whose timer throws on line 5 of `App.tsx` (in a
/// directory of its own, named `name`).
fn failing_app(name: &str, minify: bool) -> nsh_bundler::Bundle {
    let dir = std::env::temp_dir().join(format!("nsh-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    // The throw is on line 5, column 9.
    std::fs::write(
        &entry,
        "import { mount, View } from \"nsh\";\n\
         const message: string = \"boom\";\n\
         function fail(): void {\n\
         \x20 // line 4\n\
         \x20 throw new Error(message);\n\
         }\n\
         setTimeout(fail, 0);\n\
         mount(() => <View />);\n",
    )
    .unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    bundle
}

/// Runs `bundle` until it reports its first error.
fn first_error(bundle: Bundle) -> String {
    let (tx, rx) = mpsc::channel();
    let sink = Sink {
        errors: Arc::new(Mutex::new(tx)),
    };
    let config = EngineConfig {
        app: nsh_engine::App::Js(bundle),
        viewport: Size::new(100.0, 100.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
        data_dir: None,
        storage: None,
    };
    let _engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
    rx.recv_timeout(Duration::from_secs(5))
        .expect("error reported")
}

#[test]
fn uncaught_errors_point_at_app_sources() {
    let bundle = failing_app("errors", false);
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();
    // SAFETY: compiled just now by this runtime.
    let bundle = unsafe { Bundle::bytecode(bytecode) }
        .with_source_map(bundle.source_map.expect("source map"));
    let error = first_error(bundle);
    assert!(error.contains("Error: boom"), "{error}");
    assert!(error.contains("App.tsx:5:"), "not symbolicated: {error}");
    assert!(!error.contains("bundle.js"), "{error}");
}

/// Release bytecode as `nsh publish` compiles it keeps its line tables:
/// with the map in the package the device reports app source positions;
/// without it, bundle positions that the map turns into the same later.
#[test]
fn published_bytecode_keeps_positions_for_the_source_map() {
    let bundle = failing_app("published-errors", true);
    let map = bundle.source_map.clone().expect("source map");
    let bytecode = nsh_js::compile_bytecode(&bundle.code, nsh_js::Strip::Source).unwrap();

    // SAFETY: compiled just now by this runtime.
    let with_map = unsafe { Bundle::bytecode(bytecode.clone()) }.with_source_map(map.clone());
    let error = first_error(with_map);
    assert!(error.contains("App.tsx:5:"), "not symbolicated: {error}");

    // SAFETY: as above.
    let error = first_error(unsafe { Bundle::bytecode(bytecode) });
    assert!(error.contains("bundle.js:1:"), "no position: {error}");
    let mapped = nsh_engine::symbolicate(&map, &error);
    assert!(mapped.contains("App.tsx:5:"), "not symbolicated: {mapped}");
    assert!(!mapped.contains("bundle.js"), "{mapped}");

    // Without the line tables there is nothing to map.
    let stripped = nsh_js::compile_bytecode(&bundle.code, nsh_js::Strip::All).unwrap();
    // SAFETY: as above.
    let error = first_error(unsafe { Bundle::bytecode(stripped) }.with_source_map(map));
    assert!(error.contains("Error: boom"), "{error}");
    assert!(!error.contains("App.tsx:5:"), "{error}");
}
