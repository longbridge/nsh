//! Signed packages end to end: publish the counter example, verify and run
//! it, and refuse older builds afterwards.

use std::{path::PathBuf, time::Duration};

use nsh_core::{ApproxTextEngine, MountItem, Size};
use nsh_engine::{
    Engine, EngineConfig,
    package::{App, Contents, SecretKey, TrustStore, build, certify},
};

use crate::ChannelSink;

fn counter(build_number: u64, bytecode_version: &str) -> Contents {
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/counter/App.tsx");
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: true,
        split: false,
        ui_sets: vec![nsh_ui_mobile::registry()],
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, true).unwrap();
    Contents {
        app: App {
            id: "com.example.counter".into(),
            name: "Counter".into(),
            version: "1.0.0".into(),
            build: build_number,
        },
        capabilities: Some(serde_json::json!({ "network": { "hosts": ["api.example.com"] } })),
        source: bundle.code,
        bytecode: Some((bytecode, bytecode_version.into())),
        source_map: bundle.source_map,
        assets: vec![("img/dot.png".into(), vec![1, 2, 3])],
        min_runtime: None,
    }
}

fn first_texts(bundle: nsh_engine::Bundle) -> Vec<String> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: nsh_engine::App::Js(bundle),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
    };
    let _engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_verified_package_runs_and_older_builds_are_refused() {
    let dir = std::env::temp_dir().join(format!("nsh-package-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = SecretKey::generate().unwrap();
    let release = SecretKey::generate().unwrap();
    let cert = certify(
        &root,
        &release.public(),
        &["com.example.*".into()],
        0,
        u64::MAX,
    );
    let mut trust = TrustStore::open(dir.join("trust")).unwrap();
    trust.add_root(root.public());

    // Bytecode for this runtime.
    let v2 = build(&counter(2, nsh_js::BYTECODE_VERSION), &release, &cert).unwrap();
    let loaded = nsh_engine::load_package(&mut trust, &v2, &dir.join("packages")).unwrap();
    assert_eq!(loaded.app.build, 2);
    let (bundle, assets) = (loaded.bundle, loaded.assets);
    assert!(matches!(bundle.code, nsh_engine::BundleCode::Bytecode(_)));
    let access = bundle.policy.access();
    assert!(access.allows_http("GET", "https://api.example.com/x"));
    assert!(!access.allows_http("GET", "https://evil.example.org/x"));
    assert_eq!(
        std::fs::read(assets.join("img/dot.png")).unwrap(),
        [1, 2, 3]
    );
    assert!(first_texts(bundle).contains(&"0".to_owned()));

    // Bytecode for another runtime: the source runs.
    let other = build(&counter(2, "another"), &release, &cert).unwrap();
    let bundle = nsh_engine::load_package(&mut trust, &other, &dir.join("packages"))
        .unwrap()
        .bundle;
    assert!(matches!(bundle.code, nsh_engine::BundleCode::Source(_)));
    assert!(first_texts(bundle).contains(&"0".to_owned()));

    // Build 1 after build 2, also after a restart.
    let v1 = build(&counter(1, nsh_js::BYTECODE_VERSION), &release, &cert).unwrap();
    let err = nsh_engine::load_package(&mut trust, &v1, &dir.join("packages")).unwrap_err();
    assert!(err.contains("older"), "{err}");
    let mut reopened = TrustStore::open(dir.join("trust")).unwrap();
    reopened.add_root(root.public());
    assert!(nsh_engine::load_package(&mut reopened, &v1, &dir.join("packages")).is_err());

    // A tampered package never runs.
    let mut tampered = v2.clone();
    let at = tampered.len() / 2;
    tampered[at] ^= 1;
    assert!(nsh_engine::load_package(&mut trust, &tampered, &dir.join("packages")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bytecode_files_are_refused_by_read() {
    let dir = std::env::temp_dir().join(format!("nsh-read-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bundle.nshc"), [0u8; 16]).unwrap();
    std::fs::write(dir.join("bundle.js"), "1").unwrap();
    assert!(nsh_engine::Bundle::read(&dir.join("bundle.nshc")).is_err());
    assert!(nsh_engine::Bundle::read(&dir.join("bundle.js")).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}
