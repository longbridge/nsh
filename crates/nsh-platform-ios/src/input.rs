//! Text inputs: a `UITextField` (one line) or `UITextView` (`multiline`)
//! inside a container view that carries the decoration (background,
//! border, corners, shadow) like any other view.

use std::{
    cell::{Cell, RefCell},
    ffi::CStr,
};

use nsh_core::{
    AutoCapitalize, AutoFill, Bounds, Color, EditAction, EventKind, EventPayload, InputConfig,
    KeyboardAppearance, KeyboardType, NodeId, PayloadValue, Points, ReturnKeyType, SubmitBehavior,
    TextAlign,
    element::PropEnum,
    geometry::{point, px, size},
};
use nsh_engine::EngineControl;
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, ProtocolObject, Sel},
    sel,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSDictionary, NSObject, NSRange, NSString,
};
use objc2_ui_kit::{
    NSFontAttributeName, NSForegroundColorAttributeName, NSTextAlignment, UIColor, UIControl,
    UIControlEvents, UIEdgeInsets, UIKeyboardAppearance, UIKeyboardType, UILabel, UIResponder,
    UIReturnKeyType, UITextAutocapitalizationType, UITextAutocorrectionType, UITextField,
    UITextView, UIView, UIViewAutoresizing,
};

use crate::{target::EventTarget, text};

/// Limits shared by both input classes, read synchronously by delegate
/// callbacks (which cannot wait for the mounter).
pub(crate) struct InputIvars {
    insets: Cell<UIEdgeInsets>,
    /// Maximum length in UTF-16 units.
    max_length: Cell<Option<u32>>,
    /// No caret is drawn (`caret={false}`).
    caret_hidden: Cell<bool>,
    /// What return does (`InputConfig::submit`).
    submit: Cell<SubmitBehavior>,
    /// A line break typed with a modifier (Shift / Option+Return) is being
    /// inserted: it never submits.
    line_break: Cell<bool>,
    /// The caret as last reported (`onCaretMove`), in the input.
    caret: Cell<Bounds<Points>>,
    /// The app listens to pastes (`onPaste`).
    paste_listens: Cell<bool>,
    /// A paste inserts its text (`pasteBehavior`).
    paste_inserts: Cell<bool>,
    /// A built-in bar of the app's takes the text area's scroll indicator's
    /// place (`InputConfig::replaced`): its scrolling is reported.
    scroll_replaced: Cell<bool>,
    /// The scrolling last reported (offset y, content width and height,
    /// viewport width and height): only changes are reported again.
    scroll_reported: Cell<Option<[CGFloat; 5]>>,
    /// The edit menu a component that draws its own selection opened on
    /// the input (`Command::OpenEditMenu`): what it offers and what it
    /// points at, in the input. While set, the edit actions are the
    /// component's.
    edit_menu: RefCell<Option<(Vec<EditAction>, CGRect)>>,
    /// `UIEditMenuInteraction` (iOS 16), made on first use.
    edit_interaction: RefCell<Option<Retained<AnyObject>>>,
}

impl Default for InputIvars {
    fn default() -> Self {
        Self {
            insets: Cell::new(UIEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom: 0.0,
                right: 0.0,
            }),
            max_length: Cell::new(None),
            caret_hidden: Cell::new(false),
            submit: Cell::new(SubmitBehavior::Newline),
            line_break: Cell::new(false),
            caret: Cell::new(Bounds::default()),
            paste_listens: Cell::new(false),
            paste_inserts: Cell::new(true),
            scroll_replaced: Cell::new(false),
            scroll_reported: Cell::new(None),
            edit_menu: RefCell::new(None),
            edit_interaction: RefCell::new(None),
        }
    }
}

define_class!(
    /// A one-line field whose text sits inside the node's padding.
    #[unsafe(super(UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = InputIvars]
    pub(crate) struct Field;

    impl Field {
        #[unsafe(method(textRectForBounds:))]
        fn text_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }

        #[unsafe(method(editingRectForBounds:))]
        fn editing_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }

        #[unsafe(method(placeholderRectForBounds:))]
        fn placeholder_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }

        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect(&self, position: &objc2::runtime::AnyObject) -> CGRect {
            if self.ivars().caret_hidden.get() {
                return CGRect::ZERO;
            }
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), caretRectForPosition: position] }
        }

        /// Where the caret is, drawn or not (`caret()`).
        #[unsafe(method(nshCaretRectForPosition:))]
        fn shown_caret_rect(&self, position: &objc2::runtime::AnyObject) -> CGRect {
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), caretRectForPosition: position] }
        }

        /// A paste (the edit menu, Command+V): reported, and its text
        /// inserted as `pasteBehavior` says.
        #[unsafe(method(paste:))]
        fn paste(&self, sender: Option<&AnyObject>) {
            if pasting(self.as_ref(), self.ivars()) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), paste: sender] };
            }
        }

        /// Paste is offered for images and files too while the app listens;
        /// a component's edit menu offers what it asked for.
        #[unsafe(method(canPerformAction:withSender:))]
        fn can_perform(&self, action: Sel, sender: Option<&AnyObject>) -> Bool {
            if let Some(offers) = edit_menu_offers(self.ivars(), action) {
                return Bool::new(offers);
            }
            if action == sel!(paste:) && can_paste_more(self.as_ref(), self.ivars()) {
                return Bool::YES;
            }
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), canPerformAction: action, withSender: sender] }
        }

        #[unsafe(method(cut:))]
        fn cut(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::Cut) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), cut: sender] };
            }
        }

        #[unsafe(method(copy:))]
        fn copy(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::Copy) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), copy: sender] };
            }
        }

        #[unsafe(method(selectAll:))]
        fn select_all(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::SelectAll) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), selectAll: sender] };
            }
        }

        /// `UIEditMenuInteractionDelegate`: the menu keeps clear of what
        /// the component selected.
        #[unsafe(method(editMenuInteraction:targetRectForConfiguration:))]
        fn edit_menu_target_rect(&self, _interaction: &AnyObject, _configuration: &AnyObject) -> CGRect {
            self.ivars()
                .edit_menu
                .borrow()
                .as_ref()
                .map_or(CGRect::ZERO, |(_, rect)| *rect)
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            let empty = self.text().is_none_or(|t| t.length() == 0);
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), deleteBackward] };
            // Deleting in an empty field changes nothing, so no delegate
            // call reports it.
            if empty {
                backspace_on_empty(self.as_ref());
            }
        }

        /// A hardware keyboard's keys go to the app's shortcuts first
        /// (`nsh/keymap`): a key a binding takes never reaches the input.
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_began(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_ended(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_ended(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
            }
        }
    }
);

impl Area {
    /// Reports the scrolling through the delegate, the event target, which
    /// holds the engine's control.
    fn scroll_changed(&self) {
        if !self.ivars().scroll_replaced.get() {
            return;
        }
        // SAFETY: UITextView's `delegate`, set to the target.
        let delegate: Option<Retained<AnyObject>> = unsafe { msg_send![self, delegate] };
        if let Some(target) = delegate
            .as_deref()
            .and_then(|d| d.downcast_ref::<EventTarget>())
        {
            target.input_scrolled(self.as_ref());
        }
    }

    /// Adds the line break commands to `inherited` while return submits.
    fn with_line_breaks(
        &self,
        inherited: Option<Retained<NSArray<AnyObject>>>,
    ) -> Option<Retained<NSArray<AnyObject>>> {
        if self.ivars().submit.get() == SubmitBehavior::Newline {
            return inherited;
        }
        let mut commands = inherited.map(|a| a.to_vec()).unwrap_or_default();
        commands.extend(line_break_commands());
        Some(NSArray::from_retained_slice(&commands))
    }
}

/// `UIKeyCommand`s for Shift+Return and Option+Return that call
/// `nshInsertLineBreak:`, ahead of the text view's own handling of return.
fn line_break_commands() -> Vec<Retained<AnyObject>> {
    // UIKeyModifierShift, UIKeyModifierAlternate.
    const MODIFIERS: [isize; 2] = [1 << 17, 1 << 19];
    let class = AnyClass::get(c"UIKeyCommand").expect("UIKeyCommand");
    let input = NSString::from_str("\r");
    MODIFIERS
        .iter()
        .map(|&modifiers| {
            // SAFETY: UIKeyCommand's documented factory and its iOS 15
            // `wantsPriorityOverSystemBehavior` (the deployment target).
            unsafe {
                let command: Retained<AnyObject> = msg_send![
                    class,
                    keyCommandWithInput: &*input,
                    modifierFlags: modifiers,
                    action: sel!(nshInsertLineBreak:)
                ];
                let _: () = msg_send![&command, setWantsPriorityOverSystemBehavior: true];
                command
            }
        })
        .collect()
}

/// What return does in the text area behind a delegate callback, or
/// `None` when the line break comes from a modifier key (it never submits)
/// or ends an input method's composition.
pub(crate) fn area_submit(view: &UITextView) -> Option<SubmitBehavior> {
    let area = view.downcast_ref::<Area>()?;
    // SAFETY: UITextView adopts UITextInput.
    let marked: Option<Retained<AnyObject>> = unsafe { msg_send![view, markedTextRange] };
    (!area.ivars().line_break.get() && marked.is_none()).then(|| area.ivars().submit.get())
}

define_class!(
    /// A multi-line text area.
    #[unsafe(super(UITextView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = InputIvars]
    pub(crate) struct Area;

    impl Area {
        #[unsafe(method(caretRectForPosition:))]
        fn caret_rect(&self, position: &objc2::runtime::AnyObject) -> CGRect {
            if self.ivars().caret_hidden.get() {
                return CGRect::ZERO;
            }
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), caretRectForPosition: position] }
        }

        /// Where the caret is, drawn or not (`caret()`).
        #[unsafe(method(nshCaretRectForPosition:))]
        fn shown_caret_rect(&self, position: &objc2::runtime::AnyObject) -> CGRect {
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), caretRectForPosition: position] }
        }

        /// A paste (the edit menu, Command+V): reported, and its text
        /// inserted as `pasteBehavior` says.
        #[unsafe(method(paste:))]
        fn paste(&self, sender: Option<&AnyObject>) {
            if pasting(self.as_ref(), self.ivars()) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), paste: sender] };
            }
        }

        /// Paste is offered for images and files too while the app listens;
        /// a component's edit menu offers what it asked for.
        #[unsafe(method(canPerformAction:withSender:))]
        fn can_perform(&self, action: Sel, sender: Option<&AnyObject>) -> Bool {
            if let Some(offers) = edit_menu_offers(self.ivars(), action) {
                return Bool::new(offers);
            }
            if action == sel!(paste:) && can_paste_more(self.as_ref(), self.ivars()) {
                return Bool::YES;
            }
            // SAFETY: calling the superclass implementation.
            unsafe { msg_send![super(self), canPerformAction: action, withSender: sender] }
        }

        #[unsafe(method(cut:))]
        fn cut(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::Cut) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), cut: sender] };
            }
        }

        #[unsafe(method(copy:))]
        fn copy(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::Copy) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), copy: sender] };
            }
        }

        #[unsafe(method(selectAll:))]
        fn select_all(&self, sender: Option<&AnyObject>) {
            if !edit_menu_chose(self.as_ref(), self.ivars(), EditAction::SelectAll) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), selectAll: sender] };
            }
        }

        /// `UIEditMenuInteractionDelegate`: the menu keeps clear of what
        /// the component selected.
        #[unsafe(method(editMenuInteraction:targetRectForConfiguration:))]
        fn edit_menu_target_rect(&self, _interaction: &AnyObject, _configuration: &AnyObject) -> CGRect {
            self.ivars()
                .edit_menu
                .borrow()
                .as_ref()
                .map_or(CGRect::ZERO, |(_, rect)| *rect)
        }

        /// The text's size changes after edits and layout without a
        /// scroll: a built-in bar's thumb follows it.
        #[unsafe(method(setContentSize:))]
        fn set_content_size(&self, size: CGSize) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), setContentSize: size] };
            self.scroll_changed();
        }

        /// The viewport's size changes in layout.
        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            self.scroll_changed();
        }

        #[unsafe(method(deleteBackward))]
        fn delete_backward(&self) {
            let empty = self.text().length() == 0;
            // SAFETY: calling the superclass implementation.
            let _: () = unsafe { msg_send![super(self), deleteBackward] };
            if empty {
                backspace_on_empty(self.as_ref());
            }
        }

        /// Shift+Return and Option+Return on a hardware keyboard insert a
        /// line break in a text area whose return submits, as in chat
        /// apps on desktops.
        #[unsafe(method_id(keyCommands))]
        fn key_commands(&self) -> Option<Retained<NSArray<AnyObject>>> {
            // SAFETY: calling the superclass implementation.
            let inherited: Option<Retained<NSArray<AnyObject>>> =
                unsafe { msg_send![super(self), keyCommands] };
            self.with_line_breaks(inherited)
        }

        #[unsafe(method(nshInsertLineBreak:))]
        fn insert_line_break(&self, _sender: Option<&AnyObject>) {
            self.ivars().line_break.set(true);
            // SAFETY: UITextView adopts UIKeyInput.
            let _: () = unsafe { msg_send![self, insertText: &*NSString::from_str("\n")] };
            self.ivars().line_break.set(false);
        }

        /// A hardware keyboard's keys go to the app's shortcuts first
        /// (`nsh/keymap`): a key a binding takes never reaches the input.
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_began(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_ended(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &AnyObject, event: Option<&AnyObject>) {
            if !crate::keymap::input_ended(self.as_ref(), presses) {
                // SAFETY: calling the superclass implementation.
                let _: () = unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
            }
        }
    }
);

/// A paste into input `view`: reports what the general pasteboard holds
/// (`onPaste`) if the app listens; whether the input then inserts its text
/// (the superclass's `paste:`). The pasteboard is read only here, on the
/// user's paste.
fn pasting(view: &UIView, ivars: &InputIvars) -> bool {
    pasting_from(
        view,
        ivars,
        &objc2_ui_kit::UIPasteboard::generalPasteboard(),
    )
}

/// [`pasting`] from `board`.
fn pasting_from(view: &UIView, ivars: &InputIvars, board: &objc2_ui_kit::UIPasteboard) -> bool {
    if ivars.paste_listens.get() {
        let paste = crate::system::pasted(board);
        report_paste(view, paste);
    }
    // SAFETY: a plain getter (no paste notice).
    ivars.paste_inserts.get() && unsafe { board.hasStrings() }
}

/// A test's paste (`nsh test --target ios`): `content` on a pasteboard of
/// its own (the device's clipboard stays as it was), pasted into the
/// focused input as the user's paste is: reported, then its text inserted
/// as `pasteBehavior` says.
#[cfg(feature = "dev")]
pub(crate) fn test_paste(
    window: &objc2_ui_kit::UIWindow,
    content: &nsh_core::PasteEvent,
) -> Result<(), String> {
    let responder = crate::synth::first_responder(window).ok_or("no input has the focus")?;
    let (view, ivars): (&UIView, &InputIvars) =
        if let Some(field) = responder.downcast_ref::<Field>() {
            (field.as_ref(), field.ivars())
        } else if let Some(area) = responder.downcast_ref::<Area>() {
            (area.as_ref(), area.ivars())
        } else {
            return Err("no input of the app has the focus".into());
        };
    let board = objc2_ui_kit::UIPasteboard::pasteboardWithUniqueName();
    let mut items: Vec<Retained<NSDictionary<NSString, AnyObject>>> = Vec::new();
    let item = |kind: &str, value: &AnyObject| {
        NSDictionary::from_slices(&[&*NSString::from_str(kind)], &[value])
    };
    // Text and its HTML are one item, as a browser copies them.
    let mut kinds = Vec::new();
    let mut values: Vec<Retained<AnyObject>> = Vec::new();
    if !content.text.is_empty() {
        kinds.push(NSString::from_str("public.utf8-plain-text"));
        values.push(NSString::from_str(&content.text).into());
    }
    if !content.html.is_empty() {
        kinds.push(NSString::from_str("public.html"));
        values.push(NSString::from_str(&content.html).into());
    }
    if !kinds.is_empty() {
        let kinds: Vec<&NSString> = kinds.iter().map(|k| &**k).collect();
        let values: Vec<&AnyObject> = values.iter().map(|v| &**v).collect();
        items.push(NSDictionary::from_slices(&kinds, &values));
    }
    for file in &content.files {
        let kind = match file.mime.as_str() {
            "image/png" => "public.png",
            "image/jpeg" => "public.jpeg",
            "image/gif" => "com.compuserve.gif",
            _ => "public.data",
        };
        items.push(item(
            kind,
            &objc2_foundation::NSData::with_bytes(&file.data),
        ));
    }
    for uri in &content.uris {
        if let Some(url) = objc2_foundation::NSURL::URLWithString(&NSString::from_str(uri)) {
            items.push(item("public.file-url", &url));
        }
    }
    // SAFETY: UIPasteboard's setter of items (type to value).
    let _: () = unsafe { msg_send![&board, setItems: &*NSArray::from_retained_slice(&items)] };
    let inserts = pasting_from(view, ivars, &board);
    // SAFETY: plain getters of the test's pasteboard.
    if inserts && let Some(text) = unsafe { board.string() } {
        // SAFETY: UIKeyInput's method, which both input classes implement.
        let _: () = unsafe { msg_send![view, insertText: &*text] };
    }
    // SAFETY: a plain getter; the test's pasteboard is not used again.
    objc2_ui_kit::UIPasteboard::removePasteboardWithName(&*unsafe { board.name() });
    Ok(())
}

/// The ivars of input view `view` (a field or a text area).
fn ivars_of(view: &UIView) -> Option<&InputIvars> {
    view.downcast_ref::<Field>()
        .map(|f| f.ivars())
        .or_else(|| view.downcast_ref::<Area>().map(|a| a.ivars()))
}

/// What a component's edit menu offers of `action`, while it has one open
/// on the input: only what it asked for (Paste with something to paste),
/// so UIKit's other actions for the input's own text stay out.
fn edit_menu_offers(ivars: &InputIvars, action: Sel) -> Option<bool> {
    let menu = ivars.edit_menu.borrow();
    let (actions, _) = menu.as_ref()?;
    let offers = |a: EditAction| actions.contains(&a);
    let board = || objc2_ui_kit::UIPasteboard::generalPasteboard();
    Some(if action == sel!(cut:) {
        offers(EditAction::Cut)
    } else if action == sel!(copy:) {
        offers(EditAction::Copy)
    } else if action == sel!(paste:) {
        // SAFETY: plain getters, which show no paste notice.
        offers(EditAction::Paste)
            && unsafe { board().hasStrings() || board().hasImages() || board().hasURLs() }
    } else if action == sel!(selectAll:) {
        offers(EditAction::SelectAll)
    } else {
        false
    })
}

/// Reports `action` chosen from a component's edit menu (`onEditAction`),
/// if one is open on input `view`; whether it did.
fn edit_menu_chose(view: &UIView, ivars: &InputIvars, action: EditAction) -> bool {
    if ivars.edit_menu.borrow().is_none() {
        return false;
    }
    // SAFETY: both input classes have a `delegate`, set to the target.
    let delegate: Option<Retained<AnyObject>> = unsafe { msg_send![view, delegate] };
    if let Some(target) = delegate
        .as_deref()
        .and_then(|d| d.downcast_ref::<EventTarget>())
    {
        target.dispatch(
            view.tag() as NodeId,
            EventKind::EditAction,
            EventPayload::Map(vec![("action", PayloadValue::Str(action.js_name().into()))]),
        );
    }
    true
}

/// Opens the system's edit menu on input `view` over `rect` (in the
/// input), offering `actions` (`Command::OpenEditMenu`).
// `UIMenuController` before iOS 16.
#[allow(deprecated)]
pub(crate) fn open_edit_menu(view: &UIView, rect: CGRect, actions: Vec<EditAction>) {
    let Some(ivars) = ivars_of(view) else { return };
    ivars.edit_menu.replace(Some((actions, rect)));
    if !view.isFirstResponder() && !view.becomeFirstResponder() {
        return;
    }
    let interaction = AnyClass::get(c"UIEditMenuInteraction").map(|class| {
        ivars
            .edit_interaction
            .borrow_mut()
            .get_or_insert_with(|| {
                // SAFETY: `initWithDelegate:` is the interaction's
                // initializer; the input implements the delegate method it
                // answers and outlives the interaction (its view).
                let interaction: Retained<AnyObject> = unsafe {
                    let alloc: Allocated<AnyObject> = msg_send![class, alloc];
                    msg_send![alloc, initWithDelegate: view]
                };
                // SAFETY: UIView's `addInteraction:` with a UIInteraction.
                let _: () = unsafe { msg_send![view, addInteraction: &*interaction] };
                interaction
            })
            .clone()
    });
    match (interaction, AnyClass::get(c"UIEditMenuConfiguration")) {
        (Some(menu), Some(configuration)) => {
            let source = CGPoint::new(rect.origin.x + rect.size.width / 2.0, rect.origin.y);
            // SAFETY: UIEditMenuConfiguration's factory (a nil identifier)
            // and the interaction's `presentEditMenuWithConfiguration:`.
            unsafe {
                let config: Retained<AnyObject> = msg_send![
                    configuration,
                    configurationWithIdentifier: Option::<&AnyObject>::None,
                    sourcePoint: source
                ];
                // Above the selection where it fits, as a text view's menu
                // (`UIEditMenuArrowDirectionDown`); the system picks
                // another side where it does not.
                let _: () = msg_send![&*config, setPreferredArrowDirection: 2isize];
                let _: () = msg_send![&*menu, presentEditMenuWithConfiguration: &*config];
            }
        }
        _ => {
            let mtm = MainThreadMarker::from(view);
            objc2_ui_kit::UIMenuController::sharedMenuController(mtm)
                .showMenuFromView_rect(view, rect);
        }
    }
}

/// Closes the edit menu [`open_edit_menu`] opened on input `view`; the
/// edit actions are the input's own again.
#[allow(deprecated)]
pub(crate) fn close_edit_menu(view: &UIView) {
    let Some(ivars) = ivars_of(view) else { return };
    if ivars.edit_menu.take().is_none() {
        return;
    }
    if let Some(menu) = ivars.edit_interaction.borrow().as_ref() {
        // SAFETY: UIEditMenuInteraction's `dismissMenu`.
        let _: () = unsafe { msg_send![&**menu, dismissMenu] };
    } else if AnyClass::get(c"UIEditMenuInteraction").is_none() {
        let mtm = MainThreadMarker::from(view);
        objc2_ui_kit::UIMenuController::sharedMenuController(mtm).hideMenuFromView(view);
    }
}
