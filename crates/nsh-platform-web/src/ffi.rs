//! The functions the JS runtime (`web/nsh.js`) provides, as module `nsh`
//! imports. Strings and bytes cross as pointer and length into this
//! module's memory; JS copies them before returning.

#[link(wasm_import_module = "nsh")]
unsafe extern "C" {
    #[link_name = "measure_text"]
    fn js_measure_text(font: *const u8, font_len: usize, text: *const u8, text_len: usize) -> f64;
    #[link_name = "font_ascent"]
    fn js_font_ascent(font: *const u8, font_len: usize) -> f64;
    #[link_name = "font_descent"]
    fn js_font_descent(font: *const u8, font_len: usize) -> f64;
    #[link_name = "register_font"]
    fn js_register_font(
        family: *const u8,
        family_len: usize,
        data: *const u8,
        data_len: usize,
        weight: u32,
        italic: u32,
    );
    #[link_name = "submit"]
    fn js_submit(json: *const u8, len: usize);
    #[link_name = "request_frame"]
    fn js_request_frame();
    #[link_name = "report_error"]
    fn js_report_error(message: *const u8, len: usize);
    #[link_name = "log"]
    fn js_log(level: u32, message: *const u8, len: usize);
    #[link_name = "read_asset"]
    fn js_read_asset(uri: *const u8, uri_len: usize) -> u32;
    #[link_name = "fetch_start"]
    fn js_fetch_start(
        id: u32,
        method: *const u8,
        method_len: usize,
        url: *const u8,
        url_len: usize,
        headers: *const u8,
        headers_len: usize,
        body: *const u8,
        body_len: usize,
        body_kind: u32,
        follow: u32,
    );
    #[link_name = "fetch_send"]
    fn js_fetch_send(id: u32, chunk: *const u8, len: usize);
    #[link_name = "fetch_finish"]
    fn js_fetch_finish(id: u32);
    #[link_name = "fetch_pause"]
    fn js_fetch_pause(id: u32, paused: u32);
    #[link_name = "fetch_cancel"]
    fn js_fetch_cancel(id: u32);
}

/// The width of `text` in CSS `font`, in points.
pub(crate) fn measure_text(font: &str, text: &str) -> f32 {
    // SAFETY: both slices are valid for the call; JS only reads them.
    unsafe { js_measure_text(font.as_ptr(), font.len(), text.as_ptr(), text.len()) as f32 }
}

/// Ascent and descent of CSS `font` (0 when the browser cannot tell).
pub(crate) fn font_metrics(font: &str) -> (f32, f32) {
    // SAFETY: the slice is valid for the calls; JS only reads it.
    unsafe {
        (
            js_font_ascent(font.as_ptr(), font.len()) as f32,
            js_font_descent(font.as_ptr(), font.len()) as f32,
        )
    }
}

/// Adds a font (`FontFace`) to the document.
pub(crate) fn register_font(family: &str, data: &[u8], weight: u16, italic: bool) {
    // SAFETY: the slices are valid for the call; JS copies them.
    unsafe {
        js_register_font(
            family.as_ptr(),
            family.len(),
            data.as_ptr(),
            data.len(),
            weight as u32,
            italic as u32,
        )
    }
}

/// Hands one batch (JSON) to the DOM mounter.
pub(crate) fn submit(json: &str) {
    // SAFETY: the slice is valid for the call; JS copies it.
    unsafe { js_submit(json.as_ptr(), json.len()) }
}

/// Asks for a `requestAnimationFrame` callback (`nsh_frame`).
pub(crate) fn request_frame() {
    // SAFETY: no arguments.
    unsafe { js_request_frame() }
}

/// Shows an app error.
pub(crate) fn report_error(message: &str) {
    // SAFETY: the slice is valid for the call; JS copies it.
    unsafe { js_report_error(message.as_ptr(), message.len()) }
}

/// Logs to the browser's console.
pub(crate) fn log(level: log::Level, message: &str) {
    // SAFETY: the slice is valid for the call; JS copies it.
    unsafe { js_log(level as u32, message.as_ptr(), message.len()) }
}

/// The bytes of a bundled file the page preloaded (`asset:/…`), if any.
/// JS hands them back with `nsh_asset_bytes` before returning.
pub(crate) fn read_asset(uri: &str) -> Option<Vec<u8>> {
    crate::ASSET.with(|a| a.borrow_mut().take());
    // SAFETY: the slice is valid for the call; JS only reads it.
    let found = unsafe { js_read_asset(uri.as_ptr(), uri.len()) };
    if found == 0 {
        return None;
    }
    crate::ASSET.with(|a| a.borrow_mut().take())
}

/// [`fetch_start`]'s `body_kind`: no body.
pub(crate) const BODY_EMPTY: u32 = 0;
/// [`fetch_start`]'s `body_kind`: the bytes passed.
pub(crate) const BODY_BYTES: u32 = 1;
/// [`fetch_start`]'s `body_kind`: chunks to come ([`fetch_send`],
/// [`fetch_finish`]).
pub(crate) const BODY_STREAM: u32 = 2;

/// Starts request `id` (`headers`: JSON `[name, value]` pairs); what
/// happens to it comes back through the `nsh_fetch_*` exports.
pub(crate) fn fetch_start(
    id: u32,
    method: &str,
    url: &str,
    headers: &str,
    body: &[u8],
    body_kind: u32,
    follow: bool,
) {
    // SAFETY: the slices are valid for the call; JS copies them.
    unsafe {
        js_fetch_start(
            id,
            method.as_ptr(),
            method.len(),
            url.as_ptr(),
            url.len(),
            headers.as_ptr(),
            headers.len(),
            body.as_ptr(),
            body.len(),
            body_kind,
            follow as u32,
        )
    }
}

/// The next chunk of request `id`'s streamed body.
pub(crate) fn fetch_send(id: u32, chunk: &[u8]) {
    // SAFETY: the slice is valid for the call; JS copies it.
    unsafe { js_fetch_send(id, chunk.as_ptr(), chunk.len()) }
}

/// Request `id`'s streamed body is complete: JS sends the request.
pub(crate) fn fetch_finish(id: u32) {
    // SAFETY: no pointers.
    unsafe { js_fetch_finish(id) }
}

/// Stops (`true`) or resumes reading request `id`'s response body.
pub(crate) fn fetch_pause(id: u32, paused: bool) {
    // SAFETY: no pointers.
    unsafe { js_fetch_pause(id, paused as u32) }
}

/// Aborts request `id` (nothing if it ended).
pub(crate) fn fetch_cancel(id: u32) {
    // SAFETY: no pointers.
    unsafe { js_fetch_cancel(id) }
}
