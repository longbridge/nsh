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

#[test]
fn uncaught_errors_point_at_app_sources() {
    let dir = std::env::temp_dir().join(format!("nsh-errors-{}", std::process::id()));
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
        minify: false,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();
    // SAFETY: compiled just now by this runtime.
    let bundle = unsafe { Bundle::bytecode(bytecode) }
        .with_source_map(bundle.source_map.expect("source map"));

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
        storage: None,
    };
    let _engine = Engine::start(config, || ApproxTextEngine, sink).unwrap();
    let error = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("error reported");
    assert!(error.contains("Error: boom"), "{error}");
    assert!(error.contains("App.tsx:5:"), "not symbolicated: {error}");
    assert!(!error.contains("bundle.js"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}
