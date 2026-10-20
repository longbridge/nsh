//! Native navigation: stacks of screens on `UINavigationController`, and
//! modals presented as view controllers.
//!
//! The core lays screens out at their stack's size (content below the
//! header) and modals at the size reported here; this module only
//! arranges controllers and reports what the user does natively (popping
//! with the back button or a swipe, swiping a sheet down).
//!
//! UIKit asks some of this synchronously (may the swipe begin, may the back
//! button pop, may the sheet close): the answers come from props already
//! mounted (`gestureEnabled`, `preventBack`, `dismissible`), never from JS.

use std::cell::Cell;

use nsh_core::{
    Color, EdgeInsets, EventKind, EventPayload, ModalAnimation, ModalConfig, ModalPresentation,
    NodeId, PayloadValue, Points, ScreenAnimation, ScreenConfig, Size,
};
use nsh_engine::EngineControl;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};
use objc2_quartz_core::{CAMediaTiming, CATransition};
use objc2_ui_kit::{
    NSForegroundColorAttributeName, UIAdaptivePresentationControllerDelegate, UIBarButtonItem,
    UIBehavioralStyle, UIColor, UIGestureRecognizer, UIGestureRecognizerDelegate,
    UIInterfaceOrientationMask, UIModalPresentationStyle, UIModalTransitionStyle, UINavigationBar,
    UINavigationBarAppearance, UINavigationController, UINavigationControllerDelegate,
    UINavigationItem, UINavigationItemLargeTitleDisplayMode, UIPresentationController, UIResponder,
    UIView, UIViewAutoresizing, UIViewController,
};

use crate::{mount::SharedMounter, native_view::NshView};

fn insets_payload(i: EdgeInsets) -> EventPayload {
    EventPayload::Map(vec![
        ("top", PayloadValue::Number(i.top.0 as f64)),
        ("right", PayloadValue::Number(i.right.0 as f64)),
        ("bottom", PayloadValue::Number(i.bottom.0 as f64)),
        ("left", PayloadValue::Number(i.left.0 as f64)),
    ])
}

fn ui_color(c: Color) -> Retained<UIColor> {
    let (r, g, b, a) = c.to_f64();
    UIColor::colorWithRed_green_blue_alpha(r, g, b, a)
}

pub(crate) struct ScreenIvars {
    id: NodeId,
    control: EngineControl,
    /// Height of the header area the core laid the content out below.
    padded: Cell<f32>,
    /// The core lays the content out below the header: it gets the header's
    /// real height (`set_screen_header`).
    measure: Cell<bool>,
    /// The header height last reported to the core.
    measured: Cell<Option<f32>>,
    insets: Cell<Option<EdgeInsets>>,
}

define_class!(
    /// The controller of one screen; its view hosts the screen's root view.
    #[unsafe(super(UIViewController, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ScreenIvars]
    pub(crate) struct ScreenController;

    impl ScreenController {
        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            self.report_insets();
        }
    }
);

impl ScreenController {
    /// Reports the top of the view's safe area (the bottom of the header UIKit
    /// shows, wherever it put it) as the header the content starts below.
    /// The core's estimate (the bar's height plus the status bar) is wrong
    /// on Mac Catalyst, where the bar can sit in the window's title bar.
    fn report_header(&self, view: &UIView, top: f32) {
        let ivars = self.ivars();
        let header = if !ivars.measure.get() {
            None
        } else if view.window().is_some()
            && let Some(nav) = self.navigationController()
            && !nav.isNavigationBarHidden()
        {
            Some(top)
        } else {
            // Off screen or between headers: keep what was measured.
            return;
        };
        if ivars.measured.replace(header) != header {
            ivars.control.set_screen_header(ivars.id, header);
        }
    }

    fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ScreenIvars {
            id,
            control,
            padded: Cell::new(0.0),
            measure: Cell::new(false),
            measured: Cell::new(None),
            insets: Cell::new(None),
        });
        // SAFETY: UIViewController's designated initializer.
        unsafe {
            msg_send![super(this), initWithNibName: Option::<&NSString>::None, bundle: Option::<&NSObject>::None]
        }
    }

    pub(crate) fn screen_id(&self) -> NodeId {
        self.ivars().id
    }

    /// The screen's insets not covered by system UI or its header, for
    /// `useSafeAreaInsets()` inside it; and where its content starts below
    /// the header.
    pub(crate) fn report_insets(&self) {
        let Some(view) = self.view() else { return };
        let s = view.safeAreaInsets();
        self.report_header(&view, s.top as f32);
        let insets = EdgeInsets {
            top: Points((s.top as f32 - self.ivars().padded.get()).max(0.0)),
            right: Points(s.right as f32),
            bottom: Points(s.bottom as f32),
            left: Points(s.left as f32),
        };
        if self.ivars().insets.replace(Some(insets)) != Some(insets) {
            let id = self.ivars().id;
            self.ivars()
                .control
                .dispatch_event(id, EventKind::Insets, insets_payload(insets));
        }
    }
}

pub(crate) struct ModalIvars {
    id: NodeId,
    control: EngineControl,
    reported: Cell<Option<(CGSize, EdgeInsets)>>,
    /// `supportedOrientations`; empty keeps the presenter's.
    orientations: Cell<UIInterfaceOrientationMask>,
    /// `dismissible`: a swipe down closes the sheet natively.
    pub(crate) dismissible: Cell<bool>,
    /// The user closed it natively; the app has not hidden it yet.
    closed: Cell<bool>,
}

define_class!(
    /// The controller a modal is presented with.
    #[unsafe(super(UIViewController, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ModalIvars]
    pub(crate) struct ModalController;

    unsafe impl NSObjectProtocol for ModalController {}

    impl ModalController {
        #[unsafe(method(viewDidLayoutSubviews))]
        fn view_did_layout_subviews(&self) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
            self.report_size();
        }

        #[unsafe(method(viewDidAppear:))]
        fn view_did_appear(&self, animated: bool) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), viewDidAppear: animated] };
            let id = self.ivars().id;
            self.ivars().control.dispatch_event(id, EventKind::Show, EventPayload::None);
        }
    }

    impl ModalController {
        #[unsafe(method(preferredStatusBarStyle))]
        fn preferred_status_bar_style(&self) -> objc2_ui_kit::UIStatusBarStyle {
            crate::system::preferred_status_bar_style()
        }

        #[unsafe(method(prefersStatusBarHidden))]
        fn prefers_status_bar_hidden(&self) -> bool {
            crate::system::prefers_status_bar_hidden()
        }

        #[unsafe(method(preferredStatusBarUpdateAnimation))]
        fn preferred_status_bar_update_animation(&self) -> objc2_ui_kit::UIStatusBarAnimation {
            crate::system::status_bar_animation()
        }

        #[unsafe(method(supportedInterfaceOrientations))]
        fn supported_interface_orientations(&self) -> UIInterfaceOrientationMask {
            let own = self.ivars().orientations.get();
            if !own.is_empty() {
                return own;
            }
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), supportedInterfaceOrientations] }
        }
    }

    unsafe impl UIAdaptivePresentationControllerDelegate for ModalController {
        /// Answered from `dismissible`: close natively (reported when done),
        /// or let `visible` decide (a swipe down only asks).
        #[unsafe(method(presentationControllerShouldDismiss:))]
        fn should_dismiss(&self, _controller: &UIPresentationController) -> bool {
            self.ivars().dismissible.get()
        }

        /// The user swiped a `dismissible` sheet away.
        #[unsafe(method(presentationControllerDidDismiss:))]
        fn did_dismiss(&self, _controller: &UIPresentationController) {
            let ivars = self.ivars();
            ivars.closed.set(true);
            ivars
                .control
                .dispatch_event(ivars.id, EventKind::RequestClose, EventPayload::None);
            ivars
                .control
                .dispatch_event(ivars.id, EventKind::Dismiss, EventPayload::None);
        }

        #[unsafe(method(presentationControllerDidAttemptToDismiss:))]
        fn did_attempt_to_dismiss(&self, _controller: &UIPresentationController) {
            let id = self.ivars().id;
            self.ivars()
                .control
                .dispatch_event(id, EventKind::RequestClose, EventPayload::None);
        }
    }
);

impl ModalController {
    fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ModalIvars {
            id,
            control,
            reported: Cell::new(None),
            orientations: Cell::new(UIInterfaceOrientationMask::empty()),
            dismissible: Cell::new(false),
            closed: Cell::new(false),
        });
        // SAFETY: UIViewController's designated initializer.
        unsafe {
            msg_send![super(this), initWithNibName: Option::<&NSString>::None, bundle: Option::<&NSObject>::None]
        }
    }

    /// Lays the modal's content out at the size it is presented at.
    fn report_size(&self) {
        let Some(view) = self.view() else { return };
        let size = view.bounds().size;
        let s = view.safeAreaInsets();
        let insets = EdgeInsets {
            top: Points(s.top as f32),
            right: Points(s.right as f32),
            bottom: Points(s.bottom as f32),
            left: Points(s.left as f32),
        };
        if size.width <= 0.0
            || self.ivars().reported.replace(Some((size, insets))) == Some((size, insets))
        {
            return;
        }
        let ivars = self.ivars();
        ivars.control.set_root_size(
            ivars.id,
            Size::new(Points(size.width as f32), Points(size.height as f32)),
            insets,
        );
        ivars
            .control
            .dispatch_event(ivars.id, EventKind::Insets, insets_payload(insets));
    }
}

pub(crate) struct NavIvars {
    stack: NodeId,
    mounter: SharedMounter,
    /// The app's own change of screens is being applied.
    applying: Cell<bool>,
}

define_class!(
    /// A stack's navigation controller: the back button asks it before
    /// popping (`navigationBar:shouldPopItem:`), answered from `preventBack`.
    #[unsafe(super(UINavigationController, UIViewController, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavIvars]
    pub(crate) struct NavController;

    unsafe impl NSObjectProtocol for NavController {}

    impl NavController {
        /// The header's back button: a screen with `preventBack` stays and
        /// gets `onBackRequest`.
        #[unsafe(method(navigationBar:shouldPopItem:))]
        fn should_pop_item(&self, bar: &UINavigationBar, item: &UINavigationItem) -> bool {
            let ivars = self.ivars();
            if !ivars.applying.get()
                && let Ok(m) = ivars.mounter.get(self.mtm()).try_borrow()
                && let Some(m) = m.as_ref()
                && let Some(screen) = m.stack_prevents_back(ivars.stack)
            {
                m.dispatch(screen, EventKind::BackRequest);
                false
            } else {
                // SAFETY: calling the superclass implementation.
                unsafe { msg_send![super(self), navigationBar: bar, shouldPopItem: item] }
            }
        }
    }
);

impl NavController {
    fn new(mtm: MainThreadMarker, stack: NodeId, mounter: SharedMounter) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(NavIvars {
            stack,
            mounter,
            applying: Cell::new(false),
        });
        // SAFETY: UIViewController's designated initializer.
        unsafe {
            msg_send![super(this), initWithNibName: Option::<&NSString>::None, bundle: Option::<&NSObject>::None]
        }
    }
}

pub(crate) struct StackDelegateIvars {
    stack: NodeId,
    mounter: SharedMounter,
}

define_class!(
    /// Follows the navigation controller of one stack: per-screen header
    /// visibility, swipe-back permission, and screens the user popped.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = StackDelegateIvars]
    pub(crate) struct StackDelegate;

    unsafe impl NSObjectProtocol for StackDelegate {}

    unsafe impl UINavigationControllerDelegate for StackDelegate {
        #[unsafe(method(navigationController:willShowViewController:animated:))]
        fn will_show(
            &self,
            nav: &UINavigationController,
            controller: &UIViewController,
            animated: bool,
        ) {
            self.with_mounter(|m| m.stack_will_show(nav, controller, animated));
        }

        #[unsafe(method(navigationController:didShowViewController:animated:))]
        fn did_show(
            &self,
            _nav: &UINavigationController,
            _controller: &UIViewController,
            _animated: bool,
        ) {
            let stack = self.ivars().stack;
            self.with_mounter(|m| m.stack_did_show(stack));
        }
    }

    unsafe impl UIGestureRecognizerDelegate for StackDelegate {
        /// Swipe back: only with something to go back to, if the top
        /// screen allows it.
        #[unsafe(method(gestureRecognizerShouldBegin:))]
        fn should_begin(&self, _recognizer: &UIGestureRecognizer) -> bool {
            let stack = self.ivars().stack;
            let mtm = self.mtm();
            match self.ivars().mounter.get(mtm).try_borrow() {
                Ok(m) => m.as_ref().is_some_and(|m| m.stack_can_pop(stack)),
                Err(_) => false,
            }
        }
    }
);

impl StackDelegate {
    fn new(mtm: MainThreadMarker, stack: NodeId, mounter: SharedMounter) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(StackDelegateIvars { stack, mounter });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn with_mounter(&self, f: impl FnOnce(&mut crate::mount::Mounter)) {
        // Navigation callbacks can come while a batch is applied (setting
        // the view controllers): then the mounter catches up at its end.
        if let Ok(mut m) = self.ivars().mounter.get(self.mtm()).try_borrow_mut()
            && let Some(m) = m.as_mut()
        {
            f(m);
        }
    }
}

/// A navigation stack.
pub(crate) struct StackHost {
    /// Placed by the core; hosts the navigation controller's view.
    pub(crate) container: Retained<NshView>,
    pub(crate) nav: Retained<NavController>,
    _delegate: Retained<StackDelegate>,
    /// The screens, bottom to top, as the app has them.
    pub(crate) screens: Vec<NodeId>,
    /// Screens the user popped natively, until the app removes them.
    pub(crate) popped: Vec<NodeId>,
    /// The screens last given to the navigation controller (changes wait
    /// while a transition runs).
    pub(crate) applied: std::cell::RefCell<Vec<NodeId>>,
    /// The controller is a child of the surface's controller.
    contained: bool,
}

impl StackHost {
    pub(crate) fn new(mtm: MainThreadMarker, id: NodeId, mounter: SharedMounter) -> Self {
        let container = NshView::new(mtm);
        let nav = NavController::new(mtm, id, mounter.clone());
        let delegate = StackDelegate::new(mtm, id, mounter);
        // SAFETY: the delegate lives as long as the stack.
        unsafe { nav.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
        keep_in_window(&nav.navigationBar());
        if let Some(pop) = nav.interactivePopGestureRecognizer() {
            pop.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        }
        if let Some(view) = nav.view() {
            view.setAutoresizingMask(
                UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
            );
            view.setFrame(container.bounds());
            container.addSubview(&view);
        }
        Self {
            container,
            nav,
            _delegate: delegate,
            screens: Vec::new(),
            popped: Vec::new(),
            applied: Default::default(),
            contained: false,
        }
    }

    /// Makes the navigation controller a child of the controller showing
    /// the stack (appearance callbacks, safe areas, rotation).
    pub(crate) fn contain(&mut self) {
        if self.contained || self.container.window().is_none() {
            return;
        }
        let Some(parent) = controller_of(&self.container) else {
            return;
        };
        parent.addChildViewController(&self.nav);
        self.nav.didMoveToParentViewController(Some(&parent));
        self.contained = true;
    }

    /// Shows the app's screens: pushes, pops or replaces as needed.
    /// `leaving` is the transition of the screen on top until now.
    pub(crate) fn reconcile(
        &self,
        screens: &[(NodeId, Retained<ScreenController>, ScreenAnimation)],
        leaving: Option<ScreenAnimation>,
    ) {
        let desired: Vec<Retained<UIViewController>> = screens
            .iter()
            .map(|(_, c, _)| Retained::into_super(c.clone()))
            .collect();
        let current = self.nav.viewControllers();
        let same = current.count() == desired.len()
            && desired
                .iter()
                .zip(current.iter())
                .all(|(a, b)| std::ptr::eq::<UIViewController>(&**a, &*b));
        if same {
            return;
        }
        let top_changed = match (current.lastObject(), desired.last()) {
            (Some(a), Some(b)) => !std::ptr::eq::<UIViewController>(&*a, &**b),
            _ => true,
        };
        // The animation of the screen that appears (push) or leaves (pop).
        let pushing = desired.len() >= current.count();
        let animation = if pushing {
            screens.last().map(|s| s.2)
        } else {
            leaving
        }
        .unwrap_or_default();
        let animated = top_changed && current.count() > 0 && animation == ScreenAnimation::Default;
        if top_changed && current.count() > 0 && animation == ScreenAnimation::Fade {
            let fade = CATransition::new();
            fade.setDuration(0.25);
            if let Some(view) = self.nav.view() {
                view.layer().addAnimation_forKey(&fade, None);
            }
        }
        let array = NSArray::from_retained_slice(&desired);
        *self.applied.borrow_mut() = screens.iter().map(|(id, _, _)| *id).collect();
        self.nav.ivars().applying.set(true);
        self.nav.setViewControllers_animated(&array, animated);
        self.nav.ivars().applying.set(false);
    }
}

/// One screen.
pub(crate) struct ScreenHost {
    pub(crate) controller: Retained<ScreenController>,
    /// The screen's root view, placed by the core.
    pub(crate) view: Retained<NshView>,
    pub(crate) config: ScreenConfig,
}

impl ScreenHost {
    pub(crate) fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Self {
        let controller = ScreenController::new(mtm, id, control);
        let view = NshView::new(mtm);
        if let Some(root) = controller.view() {
            root.setBackgroundColor(Some(&UIColor::systemBackgroundColor()));
            root.addSubview(&view);
        }
        Self {
            controller,
            view,
            config: ScreenConfig::default(),
        }
    }

    /// Applies the header options; `item` returns the view of a header item.
    pub(crate) fn apply_header(&self, item: impl Fn(NodeId) -> Option<Retained<UIView>>) {
        let mtm = self.controller.mtm();
        let c = &self.config;
        let nav_item = self.controller.navigationItem();
        nav_item.setTitle(c.title.as_deref().map(NSString::from_str).as_deref());
        let title_view = c.title_view.and_then(&item);
        nav_item.setTitleView(title_view.as_deref());
        let bar_item = |id: Option<NodeId>| {
            id.and_then(&item)
                .map(|v| UIBarButtonItem::initWithCustomView(UIBarButtonItem::alloc(mtm), &v))
        };
        nav_item.setLeftBarButtonItem(bar_item(c.left).as_deref());
        nav_item.setLeftItemsSupplementBackButton(true);
        nav_item.setRightBarButtonItem(bar_item(c.right).as_deref());
        nav_item.setHidesBackButton(!c.back_visible);
        nav_item.setBackButtonTitle(c.back_title.as_deref().map(NSString::from_str).as_deref());
        nav_item.setLargeTitleDisplayMode(if c.large_title {
            UINavigationItemLargeTitleDisplayMode::Always
        } else {
            UINavigationItemLargeTitleDisplayMode::Never
        });
        let appearance = header_appearance(mtm, c);
        nav_item.setStandardAppearance(appearance.as_deref());
        nav_item.setScrollEdgeAppearance(appearance.as_deref());
        nav_item.setCompactAppearance(appearance.as_deref());
        // The bar of the screen on top, too (Catalyst does not always take
        // the item's appearance).
        if let Some(nav) = self.controller.navigationController()
            && nav
                .topViewController()
                .is_some_and(|top| std::ptr::eq::<UIViewController>(&*top, &**self.controller))
        {
            style_bar(&nav.navigationBar(), c);
        }
        let padded = if c.header_transparent || c.large_title {
            0.0
        } else {
            c.header_height.0
        };
        let ivars = self.controller.ivars();
        ivars.padded.set(padded);
        ivars
            .measure
            .set(c.header_shown && !c.header_transparent && !c.large_title);
        self.controller.report_insets();
    }
}

/// Keeps `bar` in the window with the iPad style: in Mac Catalyst's Mac
/// idiom UIKit otherwise moves a navigation bar's contents into the window's
/// toolbar (the back button next to the traffic lights), where the title,
/// the header's colors and the core's layout do not apply.
pub(crate) fn keep_in_window(bar: &UINavigationBar) {
    // `preferredBehavioralStyle` is iOS 16+ (nsh supports iOS 15, where
    // bars always stay in the window).
    if bar.respondsToSelector(objc2::sel!(setPreferredBehavioralStyle:)) {
        bar.setPreferredBehavioralStyle(UIBehavioralStyle::Pad);
    }
}

/// The header's appearance from the screen's options (`None`: UIKit's).
fn header_appearance(
    mtm: MainThreadMarker,
    c: &ScreenConfig,
) -> Option<Retained<UINavigationBarAppearance>> {
    if c.background_color.is_none() && c.title_color.is_none() && !c.header_transparent {
        return None;
    }
    let appearance = UINavigationBarAppearance::new(mtm);
    if c.header_transparent {
        appearance.configureWithTransparentBackground();
    } else if let Some(bg) = c.background_color {
        appearance.configureWithOpaqueBackground();
        appearance.setBackgroundColor(Some(&ui_color(bg)));
    } else {
        appearance.configureWithDefaultBackground();
    }
    if let Some(color) = c.title_color {
        let attributes = objc2_foundation::NSDictionary::from_retained_objects(
            // SAFETY: an immutable UIKit constant.
            &[unsafe { NSForegroundColorAttributeName }],
            &[Retained::into_super(Retained::into_super(ui_color(color)))],
        );
        // SAFETY: a valid title attributes dictionary.
        unsafe {
            appearance.setTitleTextAttributes(&attributes);
            appearance.setLargeTitleTextAttributes(&attributes);
        }
    }
    Some(appearance)
}

/// Styles a stack's navigation bar for the screen it shows: the screen's
/// navigation item carries the same appearance, but Mac Catalyst does not
/// always apply it, so the bar gets it as well.
pub(crate) fn style_bar(bar: &UINavigationBar, c: &ScreenConfig) {
    let mtm = bar.mtm();
    bar.setPrefersLargeTitles(c.large_title);
    let tint = c.tint_color.map(ui_color);
    // SAFETY: a plain setter; `None` restores the default tint.
    unsafe { bar.setTintColor(tint.as_deref()) };
    match header_appearance(mtm, c) {
        Some(appearance) => {
            bar.setStandardAppearance(&appearance);
            bar.setScrollEdgeAppearance(Some(&appearance));
            bar.setCompactAppearance(Some(&appearance));
        }
        None => {
            let appearance = UINavigationBarAppearance::new(mtm);
            bar.setStandardAppearance(&appearance);
            bar.setScrollEdgeAppearance(None);
            bar.setCompactAppearance(None);
        }
    }
}

/// One modal.
pub(crate) struct ModalHost {
    pub(crate) controller: Retained<ModalController>,
    /// The modal's root view, placed by the core.
    pub(crate) view: Retained<NshView>,
    pub(crate) config: ModalConfig,
    pub(crate) presented: bool,
}

impl ModalHost {
    pub(crate) fn new(mtm: MainThreadMarker, id: NodeId, control: EngineControl) -> Self {
        let controller = ModalController::new(mtm, id, control);
        let view = NshView::new(mtm);
        if let Some(root) = controller.view() {
            root.addSubview(&view);
        }
        Self {
            controller,
            view,
            config: ModalConfig::default(),
            presented: false,
        }
    }

    /// Presents the modal over the controller showing `from`.
    pub(crate) fn present(&mut self, from: &UIView) {
        if self.presented && !self.controller.ivars().closed.get() {
            return;
        }
        self.controller.ivars().closed.set(false);
        let Some(mut presenter) = controller_of(from) else {
            return log::warn!("cannot present a modal: no view controller shows the app");
        };
        while let Some(next) = presenter.presentedViewController() {
            presenter = next;
        }
        let c = self.config;
        let controller = &self.controller;
        let mut mask = UIInterfaceOrientationMask::empty();
        for (bit, m) in [
            (ModalConfig::PORTRAIT, UIInterfaceOrientationMask::Portrait),
            (
                ModalConfig::PORTRAIT_UPSIDE_DOWN,
                UIInterfaceOrientationMask::PortraitUpsideDown,
            ),
            (
                ModalConfig::LANDSCAPE_LEFT,
                UIInterfaceOrientationMask::LandscapeLeft,
            ),
            (
                ModalConfig::LANDSCAPE_RIGHT,
                UIInterfaceOrientationMask::LandscapeRight,
            ),
        ] {
            if c.orientations & bit != 0 {
                mask |= m;
            }
        }
        controller.ivars().orientations.set(mask);
        controller.ivars().dismissible.set(c.dismissible);
        controller.setModalPresentationStyle(match (c.presentation, c.transparent) {
            (_, true) => UIModalPresentationStyle::OverFullScreen,
            (ModalPresentation::FullScreen, false) => UIModalPresentationStyle::FullScreen,
            (ModalPresentation::PageSheet, false) => UIModalPresentationStyle::PageSheet,
            (ModalPresentation::FormSheet, false) => UIModalPresentationStyle::FormSheet,
        });
        // Catalyst's Mac idiom shows sheets as macOS sheets at a default
        // size that may not fit the window: fit them to it.
        let sheet = !c.transparent && c.presentation != ModalPresentation::FullScreen;
        if sheet
            && objc2_ui_kit::UIDevice::currentDevice(from.mtm()).userInterfaceIdiom()
                == objc2_ui_kit::UIUserInterfaceIdiom::Mac
            && let Some(window) = from.window()
        {
            let size = window.bounds().size;
            controller.setPreferredContentSize(objc2_core_foundation::CGSize::new(
                size.width,
                (size.height - 60.0).max(0.0),
            ));
        }
        controller.setModalTransitionStyle(match c.animation {
            ModalAnimation::Fade => UIModalTransitionStyle::CrossDissolve,
            _ => UIModalTransitionStyle::CoverVertical,
        });
        if let Some(view) = controller.view() {
            let color = if c.transparent {
                UIColor::clearColor()
            } else {
                UIColor::systemBackgroundColor()
            };
            view.setBackgroundColor(Some(&color));
        }
        if let Some(p) = controller.presentationController() {
            // SAFETY: the controller is its own delegate and outlives the
            // presentation.
            unsafe { p.setDelegate(Some(ProtocolObject::from_ref(&**controller))) };
        }
        // Presented outside the surface: the app's appearance is set on
        // it too.
        controller.setOverrideUserInterfaceStyle(crate::system::appearance());
        presenter.presentViewController_animated_completion(
            controller,
            c.animation != ModalAnimation::None,
            None,
        );
        self.presented = true;
    }

    /// Applies `config` (`dismissible` also while presented).
    pub(crate) fn configure(&mut self, config: ModalConfig) {
        self.controller.ivars().dismissible.set(config.dismissible);
        self.config = config;
    }

    /// Dismisses the modal; `done` runs when it is gone.
    pub(crate) fn dismiss(&mut self, done: impl Fn() + 'static) {
        if !self.presented {
            return;
        }
        self.presented = false;
        // Closed by the user already (and reported).
        if self.controller.ivars().closed.replace(false) {
            return;
        }
        let block = block2::RcBlock::new(done);
        self.controller.dismissViewControllerAnimated_completion(
            self.config.animation != ModalAnimation::None,
            Some(&block),
        );
    }
}

/// The view controller whose view contains `view`.
pub(crate) fn controller_of(view: &UIView) -> Option<Retained<UIViewController>> {
    let mut responder: Option<Retained<UIResponder>> = view.nextResponder();
    while let Some(r) = responder {
        if let Ok(controller) = r.clone().downcast::<UIViewController>() {
            return Some(controller);
        }
        responder = r.nextResponder();
    }
    None
}
