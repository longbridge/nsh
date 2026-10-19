//! Where an app's storage lives in a browser's `localStorage`, which the
//! origin's apps share: each entry under `nsh:<path>:<key>`, `<path>`
//! being the directory of the page's URL, so apps published at different
//! paths keep apart and the page's other keys are left alone. Here rather
//! than in the web backend, which only builds for WebAssembly, so it is
//! tested on every machine.

/// The prefix of the `localStorage` keys of the app on the page at `url`
/// (`document.baseURI`): `nsh:/apps/todo/:` for
/// `https://x.com/apps/todo/index.html?a#b`.
pub fn local_storage_prefix(url: &str) -> String {
    let url = url.split(['?', '#']).next().unwrap_or("");
    let path = match url.find("://") {
        Some(scheme) => {
            let rest = &url[scheme + 3..];
            rest.find('/').map_or("/", |host| &rest[host..])
        }
        None => url,
    };
    let dir = path.rfind('/').map_or("/", |slash| &path[..=slash]);
    format!("nsh:{dir}:")
}

#[cfg(test)]
mod tests {
    use super::local_storage_prefix as prefix;

    #[test]
    fn the_prefix_is_the_page_directory() {
        assert_eq!(
            prefix("https://x.com/apps/todo/index.html?a#b"),
            "nsh:/apps/todo/:"
        );
        // A directory and its index page share their entries.
        assert_eq!(prefix("https://x.com/apps/todo/"), "nsh:/apps/todo/:");
        assert_eq!(
            prefix("https://x.com/apps/todo/index.html"),
            "nsh:/apps/todo/:"
        );
        // Pages at other paths keep apart.
        assert_ne!(
            prefix("https://x.com/apps/todo/"),
            prefix("https://x.com/apps/notes/")
        );
        assert_eq!(prefix("https://x.com"), "nsh:/:");
        assert_eq!(prefix("https://x.com?q=/a/b"), "nsh:/:");
        assert_eq!(
            prefix("http://127.0.0.1:8765/play/index.html#counter"),
            "nsh:/play/:"
        );
        // An unknown page: one namespace.
        assert_eq!(prefix(""), "nsh:/:");
    }
}
