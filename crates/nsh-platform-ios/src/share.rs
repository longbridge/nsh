//! `Share.show`: the system's share sheet, `UIActivityViewController`
//! ([`nsh_core::share`]). The text and the link are items of their own, the
//! files file URLs; the title is the subject the first item gives (a mail's
//! subject line). On an iPad the sheet is a popover pointing at the anchor
//! the core placed in the root view, or at the middle of the surface
//! without an arrow. Its completion answers the root's `Share` event:
//! shared with the activity type the user chose, or dismissed.

use std::cell::Cell;

use block2::RcBlock;
use nsh_core::{
    EventKind, ROOT_ID,
    share::{ShareError, ShareOutcome, ShareSheet},
};
use nsh_engine::EngineControl;
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_ui_kit::{
    UIActivityItemSource, UIActivityType, UIActivityViewController, UIPopoverArrowDirection, UIView,
};

/// What one shared item holds: the item and the subject it gives.
pub(crate) struct ItemIvars {
    item: Retained<AnyObject>,
    subject: Option<Retained<NSString>>,
}

define_class!(
    /// An item of the sheet that also gives the share's title as its
    /// subject (`UIActivityItemSource`): a share sheet takes a title from
    /// nowhere else.
    #[unsafe(super(NSObject))]
    #[ivars = ItemIvars]
    pub(crate) struct ShareItem;

    unsafe impl NSObjectProtocol for ShareItem {}

    unsafe impl UIActivityItemSource for ShareItem {
        #[unsafe(method_id(activityViewControllerPlaceholderItem:))]
        fn placeholder(&self, _controller: &UIActivityViewController) -> Retained<AnyObject> {
            self.ivars().item.clone()
        }

        #[unsafe(method_id(activityViewController:itemForActivityType:))]
        fn item(
            &self,
            _controller: &UIActivityViewController,
            _activity: Option<&UIActivityType>,
        ) -> Option<Retained<AnyObject>> {
            Some(self.ivars().item.clone())
        }

        #[unsafe(method_id(activityViewController:subjectForActivityType:))]
        fn subject(
            &self,
            _controller: &UIActivityViewController,
            _activity: Option<&UIActivityType>,
        ) -> Retained<NSString> {
            self.ivars().subject.clone().unwrap_or_else(NSString::new)
        }
    }
);

impl ShareItem {
    fn new(item: Retained<AnyObject>, subject: Option<Retained<NSString>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ItemIvars { item, subject });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// `value` as the object an `NSArray` of items holds.
fn object<T: objc2::Message>(value: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an `AnyObject`.
    unsafe { Retained::cast_unchecked(value) }
}

/// Shows the share sheet for `sheet` over the controller of `root` (the
/// surface's root view) and answers request `id` once it closes.
pub(crate) fn show(id: u32, sheet: &ShareSheet, root: &UIView, control: &EngineControl) {
    let fail = |error: ShareError| {
        control.dispatch_event(ROOT_ID, EventKind::Share, error.payload(id));
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return fail(ShareError::failed("share: not on the main thread"));
    };
    let Some(top) = crate::system::top_controller(root) else {
        return fail(ShareError::failed("share: the surface is in no window"));
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
    if let Some(title) = &sheet.title {
        let first = items.remove(0);
        let item = ShareItem::new(first, Some(NSString::from_str(title)));
        items.insert(0, object(item));
    }
    let items = NSArray::from_retained_slice(&items);
    // SAFETY: the items are objects a share sheet takes (strings, URLs and
    // item sources), and no application activities are given.
    let controller = unsafe {
        UIActivityViewController::initWithActivityItems_applicationActivities(
            UIActivityViewController::alloc(mtm),
            &items,
            None,
        )
    };
    let answered = Cell::new(false);
    let control = control.clone();
    let completion = RcBlock::new(
        move |activity: *mut UIActivityType,
              completed: Bool,
              _: *mut NSArray,
              error: *mut NSError| {
            // Called once per sheet, but a second call must not answer a
            // later request of the same number.
            if answered.replace(true) {
                return;
            }
            // SAFETY: UIKit passes a valid error or null.
            let payload = match unsafe { error.as_ref() } {
                Some(error) => {
                    ShareError::failed(error.localizedDescription().to_string()).payload(id)
                }
                None if completed.as_bool() => {
                    // SAFETY: UIKit passes a valid activity type or null.
                    let target = unsafe { activity.as_ref() }.map(ToString::to_string);
                    ShareOutcome::shared(target).payload(id)
                }
                None => ShareOutcome::dismissed().payload(id),
            };
            control.dispatch_event(ROOT_ID, EventKind::Share, payload);
        },
    );
    // SAFETY: the block lives as long as UIKit keeps it (it copies it).
    unsafe { controller.setCompletionWithItemsHandler(RcBlock::as_ptr(&completion)) };
    controller.setOverrideUserInterfaceStyle(crate::system::appearance());
    if let Some(popover) = controller.popoverPresentationController() {
        // iPad and Mac: a popover pointing at the anchor, else at the
        // middle of the surface without an arrow.
        popover.setSourceView(Some(root));
        match sheet.anchor {
            Some(anchor) => popover.setSourceRect(CGRect::new(
                CGPoint::new(anchor.origin.x.0.into(), anchor.origin.y.0.into()),
                CGSize::new(anchor.size.width.0.into(), anchor.size.height.0.into()),
            )),
            None => {
                let bounds = root.bounds();
                popover.setSourceRect(CGRect::new(
                    CGPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0),
                    CGSize::ZERO,
                ));
                popover.setPermittedArrowDirections(UIPopoverArrowDirection::empty());
            }
        }
    }
    top.presentViewController_animated_completion(&controller, true, None);
}
