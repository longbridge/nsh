//! Links the prototype DLL past rustc's unresolved exports (#1644).

fn main() {
    // rustc lists 17 rquickjs `FromJs` impls in the DLL's exports without
    // generating them (#1644): link the prototype DLL past them.
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo:rustc-link-arg=/FORCE:UNRESOLVED"),
        Ok("gnu") if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") => println!("cargo:rustc-link-arg=-Wl,-Xlink=-force:unresolved"),
        _ => {}
    }
}
