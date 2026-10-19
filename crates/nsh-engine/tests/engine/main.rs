//! The engine's integration tests, one test binary: every module links the
//! bundler, QuickJS and the core, so separate binaries would each carry
//! their own copy (about a gigabyte of objects apiece in debug builds).
//!
//! Benchmarks are `#[ignore]`d tests, run in release one at a time (in
//! parallel they skew each other): `cargo test --release -p nsh-engine
//! --test engine bench_components -- --ignored --nocapture
//! --test-threads=1` (or `bench_gallery_scroll`).

use crossbeam_channel::Sender;
use nsh_core::MountBatch;
use nsh_engine::MountSink;

mod canvas;
mod class_bench;
mod compiled;
mod component_bench;
mod components;
mod counter;
mod errors;
mod fetch;
mod handle_bench;
mod hot;
mod hover;
mod js_touch;
mod keymap;
mod list;
mod list_bench;
mod native;
mod net;
mod package;
mod platform;
mod process;
mod rust_animation;
mod rust_app;
mod rust_classes;
mod rust_lists;
mod rust_platform;
mod rust_styles;
mod rust_touch;
mod shutdown;
mod split;
mod storage;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod system;
mod theme;

/// Hands the engine's batches to the test.
struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}
