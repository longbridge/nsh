//! QuickJS' local time on the page: its calls of `localtime_r` go to
//! `__wrap_localtime_r` (`src/local_time.rs`), which asks the browser,
//! instead of rquickjs' wasm shim, which answers UTC. Apps' own runtimes
//! pass the same argument (`nsh-cli` `rust_app.rs`, `WEB_BUILD_RS`).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32")
        && std::env::var_os("CARGO_FEATURE_JS").is_some()
    {
        println!("cargo:rustc-link-arg-cdylib=--wrap=localtime_r");
    }
}
