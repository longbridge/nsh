//! `nsh/base`'s `ContextMenu` given `items`, on iOS: the system's context
//! menu (`UIContextMenuInteraction`), made from the mounted items and
//! answered natively; the trigger is what the system lifts. JS hears about
//! it afterwards: `open`, then a chosen action's `select` with the app's id,
//! then `close`.

use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
};

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use nsh_core::JsonValue;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Retained, Weak},
    runtime::ProtocolObject,
};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{NSArray, NSCopying, NSObject, NSObjectProtocol, NSString};
use objc2_ui_kit::{
    UIContextMenuConfiguration, UIContextMenuInteraction, UIContextMenuInteractionAnimating,
    UIContextMenuInteractionAppearance, UIContextMenuInteractionDelegate, UIInteraction, UIMenu,
    UIMenuElement, UIPreviewParameters, UIResponder, UITargetedPreview, UIView, UIWindow,
};

use crate::{
    component::{ComponentContext, ComponentFactory, NativeComponent},
    system_menu_button::{Choose, Entry, elements, parse_items},
};

/// One opening of the menu. UIKit runs a chosen action's handler around
/// the dismissal, in no order it promises, so the choice waits here for the
/// dismissal and both are told together, the choice first.
#[derive(Default)]
struct Opening {
    /// The chosen action's id, not told yet.
    chosen: RefCell<Option<String>>,
    /// The dismissal was told: a choice now is told at once.
    ended: Cell<bool>,
}

pub(crate) struct Ivars {
    entries: RefCell<Vec<Entry>>,
    title: RefCell<String>,
    disabled: Cell<bool>,
    /// The menu is up: from `willDisplay` to `willEnd`.
    shown: Cell<bool>,
    context: ComponentContext,
    /// The interaction and the view it is on.
    interaction: RefCell<Option<(Retained<UIContextMenuInteraction>, Weak<UIView>)>>,
    /// The trigger the current menu is about: its preview.
    trigger: RefCell<Option<Weak<UIView>>>,
    /// The opening the last configuration made, until its menu ends.
    opening: RefCell<Option<Rc<Opening>>>,
}

define_class!(
    /// A `ContextMenu`'s component view: no room, no touches, the delegate
    /// of the interaction it puts on the view around the trigger.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct NshContextMenu;

    impl NshContextMenu {
        #[unsafe(method(didMoveToWindow))]
        fn did_move_to_window(&self) {
            // SAFETY: calling the superclass implementation.
            let () = unsafe { msg_send![super(self), didMoveToWindow] };
            if self.window().is_some() {
                self.attach();
            }
        }

        // Leaving the window: the interaction leaves the view it is on, which
        // may stay (the component removed while the element around it
        // stays). Its superviews are still attached here.
        #[unsafe(method(willMoveToWindow:))]
        fn will_move_to_window(&self, window: Option<&UIWindow>) {
            if window.is_none() {
                self.detach();
            }
            // SAFETY: calling the superclass implementation.
            let () = unsafe { msg_send![super(self), willMoveToWindow: window] };
        }
    }

    unsafe impl NSObjectProtocol for NshContextMenu {}

    unsafe impl UIContextMenuInteractionDelegate for NshContextMenu {
        #[unsafe(method_id(contextMenuInteraction:configurationForMenuAtLocation:))]
        fn configuration_for_menu(
            &self,
            interaction: &UIContextMenuInteraction,
            location: CGPoint,
        ) -> Option<Retained<UIContextMenuConfiguration>> {
            self.configuration(interaction, location)
        }

        #[unsafe(method_id(contextMenuInteraction:configuration:highlightPreviewForItemWithIdentifier:))]
        fn highlight_preview(
            &self,
            _interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
            _identifier: &ProtocolObject<dyn NSCopying>,
        ) -> Option<Retained<UITargetedPreview>> {
            self.preview()
        }

        #[unsafe(method_id(contextMenuInteraction:configuration:dismissalPreviewForItemWithIdentifier:))]
        fn dismissal_preview(
            &self,
            _interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
            _identifier: &ProtocolObject<dyn NSCopying>,
        ) -> Option<Retained<UITargetedPreview>> {
            self.preview()
        }

        // Before iOS 16 the system asks for the previews without an item.
        #[unsafe(method_id(contextMenuInteraction:previewForHighlightingMenuWithConfiguration:))]
        fn highlighting_preview(
            &self,
            _interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
        ) -> Option<Retained<UITargetedPreview>> {
            self.preview()
        }

        #[unsafe(method_id(contextMenuInteraction:previewForDismissingMenuWithConfiguration:))]
        fn dismissing_preview(
            &self,
            _interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
        ) -> Option<Retained<UITargetedPreview>> {
            self.preview()
        }

        #[unsafe(method(contextMenuInteraction:willDisplayMenuForConfiguration:animator:))]
        fn will_display(
            &self,
            interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
            _animator: Option<&ProtocolObject<dyn UIContextMenuInteractionAnimating>>,
        ) {
            let ivars = self.ivars();
            ivars.shown.set(true);
            // Rich: a held finger, the lifted preview; compact: the
            // secondary button's menu at the pointer.
            let by = if interaction.menuAppearance() == UIContextMenuInteractionAppearance::Compact
            {
                "pointer"
            } else {
                "touch"
            };
            ivars.context.emit("open", JsonValue::from(by));
        }

        #[unsafe(method(contextMenuInteraction:willEndForConfiguration:animator:))]
        fn will_end(
            &self,
            _interaction: &UIContextMenuInteraction,
            _configuration: &UIContextMenuConfiguration,
            animator: Option<&ProtocolObject<dyn UIContextMenuInteractionAnimating>>,
        ) {
            let ivars = self.ivars();
            ivars.shown.set(false);
            // Told once the menu went, a turn later, by when a chosen
            // action's handler has run: its `select` goes first, in the same
            // turn, so JS always hears the choice before the dismissal. A
            // handler running later still is told at once, after it.
            let (context, opening, mtm) = (ivars.context.clone(), ivars.opening.take(), self.mtm());
            let close = move || {
                let context = context.clone();
                let opening = MainThreadBound::new(opening.clone(), mtm);
                DispatchQueue::main().exec_async(move || {
                    let opening = opening.into_inner(MainThreadMarker::new().expect("main queue"));
                    if let Some(opening) = opening {
                        opening.ended.set(true);
                        if let Some(id) = opening.chosen.take() {
                            context.emit("select", JsonValue::from(id));
                        }
                    }
                    context.emit("close", JsonValue::Null);
                });
            };
            match animator {
                Some(animator) => animator.addCompletion(&RcBlock::new(close)),
                None => close(),
            }
        }
    }
);

impl NshContextMenu {
    fn new(mtm: MainThreadMarker, context: ComponentContext) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            entries: RefCell::default(),
            title: RefCell::default(),
            disabled: Cell::new(false),
            shown: Cell::new(false),
            context,
            interaction: RefCell::default(),
            trigger: RefCell::default(),
            opening: RefCell::default(),
        });
        // SAFETY: UIView's designated initializer.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        this.setUserInteractionEnabled(false);
        this
    }

    /// The view the interaction goes on: the one around the component's
    /// container, which holds the trigger.
    fn host(&self) -> Option<(Retained<UIView>, Retained<UIView>)> {
        let container = self.superview()?;
        let host = container.superview()?;
        Some((host, container))
    }

    /// Puts the interaction on the view around the trigger, moving it there
    /// from where it was.
    fn attach(&self) {
        let Some((host, _)) = self.host() else { return };
        let ivars = self.ivars();
        let on_host = ivars
            .interaction
            .borrow()
            .as_ref()
            .and_then(|(_, view)| view.load())
            .is_some_and(|view| std::ptr::eq::<UIView>(&*view, &*host));
        if on_host {
            return;
        }
        self.detach();
        let mtm = self.mtm();
        let interaction = UIContextMenuInteraction::initWithDelegate(
            UIContextMenuInteraction::alloc(mtm),
            ProtocolObject::from_ref(self),
        );
        // SAFETY: UIView's `addInteraction:` with a UIInteraction; the view
        // keeps it, the interaction keeps its delegate weakly.
        let () = unsafe { msg_send![&*host, addInteraction: &*interaction] };
        ivars
            .interaction
            .replace(Some((interaction, Weak::from_retained(&host))));
    }

    fn detach(&self) {
        if let Some((interaction, view)) = self.ivars().interaction.take()
            && let Some(view) = view.load()
        {
            // SAFETY: UIView's `removeInteraction:` with one it holds.
            let () = unsafe { msg_send![&*view, removeInteraction: &*interaction] };
        }
    }

    /// The menu about the trigger at `location` (in the interaction's view),
    /// none when the menu is disabled or empty or nothing is there.
    fn configuration(
        &self,
        interaction: &UIContextMenuInteraction,
        location: CGPoint,
    ) -> Option<Retained<UIContextMenuConfiguration>> {
        let ivars = self.ivars();
        if ivars.disabled.get() || ivars.entries.borrow().is_empty() {
            return None;
        }
        let host = interaction.view()?;
        let (_, container) = self.host()?;
        let trigger = trigger_at(&host, &container, location)?;
        ivars.trigger.replace(Some(Weak::from_retained(&trigger)));
        let mtm = self.mtm();
        let opening = Rc::new(Opening::default());
        ivars.opening.replace(Some(opening.clone()));
        let context = ivars.context.clone();
        let choose: Choose = Rc::new(move |id| {
            if opening.ended.get() {
                context.emit("select", JsonValue::from(id));
            } else {
                opening.chosen.replace(Some(id));
            }
        });
        let children = elements(&ivars.entries.borrow(), false, &choose, mtm);
        let menu = UIMenu::menuWithTitle_children(
            &NSString::from_str(&ivars.title.borrow()),
            &children,
            mtm,
        );
        // The menu is made now, from the items mounted beforehand; the
        // system's suggested elements are left out, as the app's menu is
        // all it shows.
        let provider = RcBlock::new(move |_: NonNull<NSArray<UIMenuElement>>| {
            Retained::as_ptr(&menu).cast_mut()
        });
        // SAFETY: no identifier (the system makes one), no preview
        // controller (the trigger is the preview: `preview`), and an action
        // provider block the configuration copies; the menu it returns is
        // kept alive by the block, and the system retains it.
        let configuration = unsafe {
            UIContextMenuConfiguration::configurationWithIdentifier_previewProvider_actionProvider(
                None,
                std::ptr::null_mut(),
                RcBlock::as_ptr(&provider),
                mtm,
            )
        };
        Some(configuration)
    }

    /// The trigger as the system lifts it: itself, clipped to its outline
    /// (each corner as drawn), on the system's platter behind what it
    /// leaves transparent.
    fn preview(&self) -> Option<Retained<UITargetedPreview>> {
        let trigger = self.ivars().trigger.borrow().as_ref()?.load()?;
        trigger.window()?;
        let mtm = self.mtm();
        let parameters = UIPreviewParameters::new(mtm);
        parameters.setVisiblePath(Some(&crate::decor::outline(&trigger)));
        Some(UITargetedPreview::initWithView_parameters(
            UITargetedPreview::alloc(mtm),
            &trigger,
            &parameters,
        ))
    }
}

/// The trigger at `location` in `host`: its topmost subview there, other
/// than the component's own container.
fn trigger_at(host: &UIView, container: &UIView, location: CGPoint) -> Option<Retained<UIView>> {
    let views: Vec<Retained<UIView>> = host.subviews().iter().collect();
    views.into_iter().rev().find(|view| {
        !std::ptr::eq::<UIView>(&**view, container)
            && !view.isHidden()
            && contains(view.frame(), location)
    })
}

fn contains(frame: objc2_core_foundation::CGRect, p: CGPoint) -> bool {
    p.x >= frame.origin.x
        && p.y >= frame.origin.y
        && p.x < frame.origin.x + frame.size.width
        && p.y < frame.origin.y + frame.size.height
}

struct SystemContextMenu {
    view: Retained<NshContextMenu>,
}

impl NativeComponent for SystemContextMenu {
    fn view(&self) -> &UIView {
        &self.view
    }

    fn set_prop(&mut self, name: &str, value: Option<&JsonValue>) {
        let ivars = self.view.ivars();
        match name {
            // A menu shown keeps the items it opened with; new ones are the
            // next opening's.
            "items" => {
                ivars.entries.replace(parse_items(
                    value.and_then(JsonValue::as_array).map_or(&[], |v| v),
                ));
            }
            "title" => {
                ivars.title.replace(
                    value
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
            "disabled" => {
                ivars
                    .disabled
                    .set(value.and_then(JsonValue::as_bool).unwrap_or(false));
            }
            // The app closing it; the system has no way to open one.
            "open" => {
                if value.and_then(JsonValue::as_bool) == Some(false)
                    && ivars.shown.get()
                    && let Some((interaction, _)) = &*ivars.interaction.borrow()
                {
                    interaction.dismissMenu();
                }
            }
            _ => {}
        }
    }
}

/// Makes the component of each `ContextMenu` given `items`
/// (`nsh_core::platform::CONTEXT_MENU_COMPONENT`), which every surface has
/// (`Components::default`).
pub(crate) fn factory() -> ComponentFactory {
    Rc::new(|context: ComponentContext| {
        let mtm = MainThreadMarker::new().expect("the context menu is made on the main thread");
        Box::new(SystemContextMenu {
            view: NshContextMenu::new(mtm, context),
        }) as Box<dyn NativeComponent>
    })
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    use super::contains;

    #[test]
    fn a_frame_holds_its_start_but_not_its_end() {
        let frame = CGRect::new(CGPoint::new(10.0, 20.0), CGSize::new(100.0, 40.0));
        assert!(contains(frame, CGPoint::new(10.0, 20.0)));
        assert!(contains(frame, CGPoint::new(109.0, 59.0)));
        assert!(!contains(frame, CGPoint::new(110.0, 30.0)));
        assert!(!contains(frame, CGPoint::new(50.0, 19.0)));
    }
}
