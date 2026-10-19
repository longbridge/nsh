//! `nsh/clipboard`: the system clipboard's text, image and files
//! (UIPasteboard, ClipboardManager, NSPasteboard, `navigator.clipboard`,
//! ...). Needs the `clipboard` capability when the app declares
//! capabilities.

use crate::ui::{ClipboardImage, ModuleCx, module};

/// The system clipboard's text, image and files.
#[module]
pub(crate) struct Clipboard;

#[module]
impl Clipboard {
    /// Puts `text` on the clipboard.
    pub(crate) fn set_string(text: String, cx: &mut ModuleCx) {
        cx.set_clipboard(text);
    }

    /// The clipboard's text, `""` when it holds none. On iOS, reading it
    /// may show the system's paste notice.
    pub(crate) async fn get_string() -> String {
        crate::ui::prelude::clipboard_string().await
    }

    /// Whether the clipboard holds text, without reading it (no paste
    /// notice).
    pub(crate) async fn has_string() -> bool {
        crate::ui::prelude::clipboard_has_string().await
    }

    /// Puts the image of `uri`, a `data:` URI (as `getImage` gives), on the
    /// clipboard, replacing what it holds.
    pub(crate) fn set_image(uri: String, cx: &mut ModuleCx) {
        cx.set_clipboard_image(uri);
    }

    /// The clipboard's image, `null` when it holds none: `uri` is a
    /// `data:` URI an `Image` shows and `fetch(uri)` reads the bytes of,
    /// `width` and `height` its size in pixels. On iOS, reading it may show
    /// the system's paste notice.
    pub(crate) async fn get_image() -> Option<ClipboardImage> {
        crate::ui::prelude::clipboard_image().await
    }

    /// Whether the clipboard holds an image, without reading it (no paste
    /// notice).
    pub(crate) async fn has_image() -> bool {
        crate::ui::prelude::clipboard_has_image().await
    }

    /// The `file:` URLs of the local files on the clipboard (files copied in
    /// a file manager), `[]` when it holds none.
    pub(crate) async fn get_files() -> Vec<String> {
        crate::ui::prelude::clipboard_files().await
    }
}
