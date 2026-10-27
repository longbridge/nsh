use nsh_core::{Bounds, EventMask, NodeId, Points, TextLayout, ViewKind, ViewProps, ViewStyle};

use crate::NativeComponent;

pub(crate) struct NshView {
    pub(crate) kind: ViewKind,
    pub(crate) concealed: bool,
    pub(crate) stack: Option<Box<nsh_core::navigation::Stack>>,
    pub(crate) screen: Option<Box<crate::navigation::Screen>>,
    pub(crate) modal: Option<Box<crate::navigation::Modal>>,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) frame: Bounds<Points>,
    pub(crate) style: ViewStyle,
    pub(crate) props: ViewProps,
    pub(crate) events: EventMask,
    pub(crate) tracked: u8,
    pub(crate) states: u8,
    pub(crate) input: Option<Box<nsh_core::InputConfig>>,
    pub(crate) field: Option<Box<crate::text_input::Field>>,
    pub(crate) text: Option<(TextLayout, Bounds<Points>)>,
    pub(crate) scroll: Option<Box<nsh_core::scroll::ScrollState>>,
    pub(crate) canvas: Option<Box<crate::canvas::CanvasView>>,
    pub(crate) component: Option<Box<dyn NativeComponent>>,
    pub(crate) component_name: Option<String>,
}

impl NshView {
    pub(crate) fn new(kind: ViewKind) -> Self {
        Self {
            kind,
            concealed: matches!(kind, ViewKind::Screen | ViewKind::HeaderItem),
            stack: (kind == ViewKind::Stack).then(Default::default),
            screen: (kind == ViewKind::Screen).then(Default::default),
            modal: (kind == ViewKind::Modal).then(Default::default),
            parent: None,
            children: Vec::new(),
            frame: Default::default(),
            style: Default::default(),
            props: Default::default(),
            events: Default::default(),
            tracked: 0,
            states: 0,
            input: None,
            field: (kind == ViewKind::Input).then(Default::default),
            text: None,
            scroll: (kind == ViewKind::Scroll).then(Default::default),
            canvas: None,
            component: None,
            component_name: None,
        }
    }
}
