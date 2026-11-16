//! `Share.show`: AppKit's share menu, `NSSharingServicePicker`
//! ([`nsh_core::share`]). The text, the link and the files' URLs are its
//! items; the title becomes the subject of the service the user chooses (a
//! mail's subject line). The menu shows next to the anchor the core placed
//! in the root view (flipped, as the core's coordinates), or at the middle
//! of the surface. Choosing a service answers the root's `Share` event as
//! shared, with the service's title; closing the menu answers dismissed.

use std::{cell::RefCell, rc::Rc};

use nsh_core::{
    EventKind, ROOT_ID,
    share::{ShareError, ShareOutcome, ShareSheet},
};
use nsh_engine::EngineControl;
use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
};
use objc2_app_kit::NSView;
use objc2_foundation::{
    NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

use crate::surface::Surface;

/// `NSRectEdgeMaxY`: below the anchor in a flipped view.
const BELOW: isize = 3;

thread_local! {
    /// The picker showing and its delegate (the picker holds it weakly),
    /// kept until the user chooses or closes it.
    static SHOWING: RefCell<Option<(Retained<AnyObject>, Retained<PickerDelegate>)>> =
        const { RefCell::new(None) };
}

pub(crate) struct DelegateIvars {
    id: u32,
    control: EngineControl,
    title: Option<Retained<NSString>>,
}

define_class!(
    /// The delegate of one share menu: gives the chosen service the title
    /// as its subject and reports what the user chose.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    #[name = "NshSharePickerDelegate"]
    pub(crate) struct PickerDelegate;

    unsafe impl NSObjectProtocol for PickerDelegate {}

    impl PickerDelegate {
        #[unsafe(method(sharingServicePicker:delegateForSharingService:))]
        fn delegate_for(&self, _picker: &AnyObject, service: &AnyObject) -> *mut AnyObject {
            if let Some(title) = &self.ivars().title {
                // SAFETY: NSSharingService's documented `subject` setter.
                let _: () = unsafe { msg_send![service, setSubject: &**title] };
            }
            std::ptr::null_mut()
        }

        #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
        fn did_choose(&self, _picker: &AnyObject, service: Option<&AnyObject>) {
            let DelegateIvars { id, control, .. } = self.ivars();
            let outcome = match service {
                Some(service) => {
                    // SAFETY: NSSharingService's documented `title`.
                    let title: Option<Retained<NSString>> = unsafe { msg_send![service, title] };
                    ShareOutcome::shared(title.map(|title| title.to_string()))
                }
                None => ShareOutcome::dismissed(),
            };
            control.dispatch_event(ROOT_ID, EventKind::Share, outcome.payload(*id));
            // The picker is done: let it and this delegate go once AppKit
            // returns from this call.
            if let Some(showing) = SHOWING.with_borrow_mut(Option::take) {
                release_later(showing);
            }
        }
    }
);

/// Releases `showing` on a later turn of the main run loop, not inside the
/// delegate call AppKit is making on it.
fn release_later(showing: (Retained<AnyObject>, Retained<PickerDelegate>)) {
    let held = RefCell::new(Some(showing));
    let block = block2::RcBlock::new(move || drop(held.borrow_mut().take()));
    // SAFETY: NSRunLoop's documented `performBlock:` on the main run loop.
    unsafe {
        let run_loop: Retained<AnyObject> = msg_send![objc2::class!(NSRunLoop), mainRunLoop];
        let _: () = msg_send![&run_loop, performBlock: &*block];
    }
}

/// Shows the share menu for `sheet` in `surface` and answers request `id`.
pub(crate) fn show(surface: &Rc<Surface>, id: u32, sheet: &ShareSheet) {
    let Some(control) = surface.event_control() else {
        return;
    };
    let fail = |error: ShareError| {
        control.dispatch_event(ROOT_ID, EventKind::Share, error.payload(id));
    };
    let Some(class) = AnyClass::get(c"NSSharingServicePicker") else {
        return fail(ShareError::unavailable("this system has no share menu"));
    };
    let mut items: Vec<Retained<AnyObject>> = Vec::new();
    if let Some(text) = &sheet.text {
        items.push(object(NSString::from_str(text)));
    }
    if let Some(url) = &sheet.url {
        match NSURL::URLWithString(&NSString::from_str(url)) {
            Some(url) => items.push(object(url)),
            None => return fail(ShareError::failed(format!("share: not a URL ({url})"))),
        }
    }
    for path in &sheet.files {
        items.push(object(NSURL::fileURLWithPath(&NSString::from_str(path))));
    }
    if items.is_empty() {
        return fail(ShareError::failed("share: nothing to share"));
    }
    let view: &NSView = &surface.root;
    let rect = match sheet.anchor {
        Some(anchor) => NSRect::new(
            NSPoint::new(anchor.origin.x.0.into(), anchor.origin.y.0.into()),
            NSSize::new(anchor.size.width.0.into(), anchor.size.height.0.into()),
        ),
        None => {
            let bounds = view.bounds();
            NSRect::new(
                NSPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0),
                NSSize::new(0.0, 0.0),
            )
        }
    };
    // A menu still showing is answered as closed: one shows at a time.
    if let Some((_, delegate)) = SHOWING.with_borrow_mut(Option::take) {
        let ivars = delegate.ivars();
        ivars.control.dispatch_event(
            ROOT_ID,
            EventKind::Share,
            ShareOutcome::dismissed().payload(ivars.id),
        );
    }
    let delegate = PickerDelegate::alloc(surface.mtm).set_ivars(DelegateIvars {
        id,
        control: control.clone(),
        title: sheet.title.as_deref().map(NSString::from_str),
    });
    // SAFETY: NSObject's designated initializer.
    let delegate: Retained<PickerDelegate> = unsafe { msg_send![super(delegate), init] };
    let items = NSArray::from_retained_slice(&items);
    // SAFETY: NSSharingServicePicker's documented API (macOS 10.8), on the
    // main thread; its items are strings and URLs, and the delegate is kept
    // in `SHOWING` while it shows.
    unsafe {
        let picker: Allocated<AnyObject> = msg_send![class, alloc];
        let picker: Option<Retained<AnyObject>> = msg_send![picker, initWithItems: &*items];
        let Some(picker) = picker else {
            return fail(ShareError::failed(
                "share: the share menu could not be made",
            ));
        };
        let _: () = msg_send![&picker, setDelegate: &*delegate];
        // Kept before it shows: AppKit may report the choice before
        // `showRelativeToRect` returns.
        SHOWING.with_borrow_mut(|showing| *showing = Some((picker.clone(), delegate)));
        let _: () =
            msg_send![&picker, showRelativeToRect: rect, ofView: view, preferredEdge: BELOW];
    }
}

/// `value` as the object an `NSArray` of items holds.
fn object<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an `AnyObject`.
    unsafe { Retained::cast_unchecked(value) }
}
