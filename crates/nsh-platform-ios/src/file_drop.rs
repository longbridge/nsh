//! File drops (`onFileDragEnter`, `onFileDragLeave`, `onFileDrop`) on iOS:
//! a `UIDropInteraction` on each view that listens to `onFileDrop`, whose
//! delegate is the surface's [`crate::target::EventTarget`].
//!
//! UIKit routes a drag to the deepest view under it with a drop interaction
//! that can handle the session, so the closest listener takes the files, as
//! the desktop backends' hit testing gives them. A text input keeps the text
//! drags that come to it; a drag of files is no text, and goes to the
//! listener around the input.
//!
//! A drag of another app's items carries no file URLs: an item offers its
//! data in types (a photo as `public.jpeg`, a document as `com.adobe.pdf`),
//! read only when it is dropped. So `onFileDragEnter` reports no URLs here,
//! and the drop copies each item's file into the app's temporary directory
//! (the provider deletes its own copy when the read returns), then reports
//! those copies' `file:` URLs, in the drag's order. An item that offers only
//! text or a link is no file: it is not offered to the listener.

use std::{
    ffi::CStr,
    sync::{Arc, Mutex},
};

use block2::RcBlock;
use objc2::{
    msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};

/// `UIDropOperationCopy`.
const COPY: isize = 2;
/// `UIDropOperationForbidden`.
const FORBIDDEN: isize = 1;

fn class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// A drop interaction whose delegate is `delegate` (the surface's event
/// target), for a view that listens to `onFileDrop`.
pub(crate) fn interaction(delegate: &AnyObject) -> Option<Retained<AnyObject>> {
    let class = class(c"UIDropInteraction")?;
    // SAFETY: UIDropInteraction's documented initializer (iOS 11); it holds
    // its delegate weakly, and the event target outlives the surface's views.
    unsafe {
        let alloc: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![alloc, initWithDelegate: delegate]
    }
}

/// The type identifier of the file `provider` offers: its first registered
/// type that is data, but not text or a link (those are a text drag's).
fn file_type(provider: &AnyObject) -> Option<Retained<NSString>> {
    let type_class = crate::picker::uniform_types()
        .then(|| class(c"UTType"))
        .flatten()?;
    // SAFETY: NSItemProvider's and UTType's documented API (iOS 11, 14).
    unsafe {
        let types: Retained<NSArray<NSString>> = msg_send![provider, registeredTypeIdentifiers];
        let named = |identifier: &str| -> Option<Retained<AnyObject>> {
            let identifier = NSString::from_str(identifier);
            msg_send![type_class, typeWithIdentifier: &*identifier]
        };
        let (data, text, url) = (
            named("public.data")?,
            named("public.text")?,
            named("public.url")?,
        );
        types.iter().find(|identifier| {
            let found: Option<Retained<AnyObject>> =
                msg_send![type_class, typeWithIdentifier: &**identifier];
            found.is_some_and(|t| {
                let conforms = |to: &AnyObject| -> bool { msg_send![&t, conformsToType: to] };
                conforms(&data) && !conforms(&text) && !conforms(&url)
            })
        })
    }
}

/// The item providers of `session`'s items (a `UIDropSession`).
fn providers(session: &AnyObject) -> Vec<Retained<AnyObject>> {
    // SAFETY: UIDragDropSession's `items`, UIDragItem's `itemProvider`.
    unsafe {
        let items: Retained<NSArray<AnyObject>> = msg_send![session, items];
        items
            .iter()
            .map(|item| msg_send![&item, itemProvider])
            .collect()
    }
}

/// Whether `session` offers at least one file.
pub(crate) fn offers_files(session: &AnyObject) -> bool {
    providers(session).iter().any(|p| file_type(p).is_some())
}

/// The proposal for `session` over a listener: copy its files.
pub(crate) fn proposal(session: &AnyObject) -> Option<Retained<AnyObject>> {
    let operation = if offers_files(session) {
        COPY
    } else {
        FORBIDDEN
    };
    let class = class(c"UIDropProposal")?;
    // SAFETY: UIDropProposal's documented initializer (iOS 11).
    unsafe {
        let alloc: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![alloc, initWithDropOperation: operation]
    }
}

/// Where `session` is, in the window's coordinates, or `None` once
/// `interaction`'s view is gone. A drop session takes no nil view
/// (`locationInView:` raises), so the point is read in the view's
/// coordinates and converted.
pub(crate) fn location(
    interaction: &AnyObject,
    session: &AnyObject,
) -> Option<objc2_core_foundation::CGPoint> {
    // SAFETY: UIInteraction's `view` (weak), UIDragDropSession's
    // `locationInView:` with that view.
    unsafe {
        let view: Option<Retained<objc2_ui_kit::UIView>> = msg_send![interaction, view];
        let view = view?;
        let local: objc2_core_foundation::CGPoint = msg_send![session, locationInView: &*view];
        Some(view.convertPoint_toView(local, None))
    }
}

/// The directory this drop's copies go to: a new one in the app's
/// temporary directory, so files of the same name do not collide.
fn drop_directory() -> Option<std::path::PathBuf> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join("nsh-drops").join(format!(
        "{}-{n}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    ));
    std::fs::create_dir_all(&dir).ok().map(|()| dir)
}

/// The file name of an item's copy: the provider's suggested name with the
/// extension of the file it wrote, or that file's own name.
fn file_name(suggested: &str, written: &std::path::Path) -> String {
    let own = written
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let suggested = suggested.trim().replace('/', "_");
    if suggested.is_empty() {
        return own;
    }
    match written.extension().map(|e| e.to_string_lossy()) {
        Some(ext) if !suggested.ends_with(&format!(".{ext}")) => format!("{suggested}.{ext}"),
        _ => suggested,
    }
}

/// Reads the files `session` offers into the app's temporary directory, then
/// calls `done` with their `file:` URLs in the drag's order (on a queue of
/// the providers'; empty when none could be read).
pub(crate) fn read(session: &AnyObject, done: impl FnOnce(Vec<String>) + Send + 'static) {
    let files: Vec<_> = providers(session)
        .into_iter()
        .filter_map(|p| file_type(&p).map(|t| (p, t)))
        .collect();
    let Some(dir) = (!files.is_empty()).then(drop_directory).flatten() else {
        done(Vec::new());
        return;
    };
    struct Reading<F> {
        uris: Vec<Option<String>>,
        left: usize,
        done: Option<F>,
    }
    let reading = Arc::new(Mutex::new(Reading {
        uris: vec![None; files.len()],
        left: files.len(),
        done: Some(done),
    }));
    for (index, (provider, kind)) in files.into_iter().enumerate() {
        // SAFETY: NSItemProvider's `suggestedName` (iOS 11).
        let suggested: Option<Retained<NSString>> = unsafe { msg_send![&provider, suggestedName] };
        let suggested = suggested.map(|n| n.to_string()).unwrap_or_default();
        let item_dir = dir.join(index.to_string());
        let reading = reading.clone();
        let handler = RcBlock::new(move |url: *mut NSURL, _error: *mut NSError| {
            // SAFETY: the URL of the file the provider wrote, valid until
            // this returns (null on failure).
            let url = unsafe { url.as_ref() };
            let uri = url
                .and_then(|url| url.path())
                .and_then(|path| {
                    let written = std::path::PathBuf::from(path.to_string());
                    let to = item_dir.join(file_name(&suggested, &written));
                    std::fs::create_dir_all(&item_dir).ok()?;
                    std::fs::copy(&written, &to).ok()?;
                    Some(to)
                })
                .and_then(|to| {
                    let path = NSString::from_str(&to.to_string_lossy());
                    NSURL::fileURLWithPath(&path).absoluteString()
                })
                .map(|uri| uri.to_string());
            if uri.is_none() {
                log::warn!("a dropped file could not be read");
            }
            let mut state = reading.lock().unwrap_or_else(|e| e.into_inner());
            state.uris[index] = uri;
            state.left -= 1;
            if state.left == 0
                && let Some(done) = state.done.take()
            {
                let uris = state.uris.drain(..).flatten().collect();
                drop(state);
                done(uris);
            }
        });
        // SAFETY: NSItemProvider's documented API (iOS 11); the handler runs
        // once on a queue of the provider's.
        let _: Option<Retained<AnyObject>> = unsafe {
            msg_send![&provider, loadFileRepresentationForTypeIdentifier: &*kind, completionHandler: &*handler]
        };
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::file_name;

    #[test]
    fn copies_keep_the_suggested_name_with_the_written_extension() {
        let written = Path::new("/tmp/x/IMG_0001.jpeg");
        assert_eq!(file_name("Beach", written), "Beach.jpeg");
        assert_eq!(file_name("Beach.jpeg", written), "Beach.jpeg");
        assert_eq!(file_name("", written), "IMG_0001.jpeg");
        assert_eq!(file_name("a/b", written), "a_b.jpeg");
        assert_eq!(file_name("Notes", Path::new("/tmp/x/notes")), "Notes");
    }
}
