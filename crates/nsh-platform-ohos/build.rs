//! Configures the OHOS N-API linker when cross compiling the native library.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("ohos") {
        napi_build_ohos::setup();
        // khronos-egl with no-pkg-config leaves the platform library to its host.
        println!("cargo:rustc-link-lib=EGL");
    }
}
