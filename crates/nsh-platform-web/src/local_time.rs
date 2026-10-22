//! QuickJS' local time zone is the browser's. QuickJS reads the offset of
//! local time from `localtime_r` (quickjs.c `getTimezoneOffset`: only
//! `tm_gmtoff`), which rquickjs' wasm shim answers with zeros (UTC); the
//! link argument of `build.rs` sends its calls here, and the page answers
//! for the time asked about (`nsh.js` `localOffset`), so daylight saving
//! time is the browser's too.

/// wasi-libc's `struct tm` (`__struct_tm.h` in rquickjs-sys' vendored
/// headers).
#[repr(C)]
struct Tm {
    fields: [i32; 9],
    gmtoff: i32,
    zone: *const u8,
    nsec: i32,
}

#[link(wasm_import_module = "nsh")]
unsafe extern "C" {
    /// The offset of local time at `ms` in seconds east of UTC (`nsh.js`
    /// `localOffset`, which `local-time.test.mjs` tests in several zones).
    #[link_name = "local_offset"]
    fn js_local_offset(ms: f64) -> f64;
}

/// `localtime_r` for QuickJS: only the offset east of UTC, in seconds, is
/// set; the other fields stay zero, as the shim it replaces left them.
///
/// # Safety
///
/// `t` and `tm` are valid, as libc's `localtime_r` requires.
#[unsafe(no_mangle)]
unsafe extern "C" fn __wrap_localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm {
    // SAFETY: the caller passes a valid `time_t`.
    let seconds = unsafe { *t };
    // SAFETY: a pure host function.
    let east = unsafe { js_local_offset(seconds as f64 * 1000.0) };
    let gmtoff = if east.is_finite() { east as i32 } else { 0 };
    // SAFETY: the caller passes a valid, writable `struct tm`.
    unsafe {
        tm.write(Tm {
            fields: [0; 9],
            gmtoff,
            zone: std::ptr::null(),
            nsec: 0,
        });
    }
    tm
}
