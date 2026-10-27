use nsh_core::{Bounds, EventMask, NodeId, Points, TextLayout, ViewKind, ViewProps, ViewStyle};

use crate::NativeComponent;

pub(crate) struct NshView {
    pub(crate) kind: ViewKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) frame: Bounds<Points>,
    pub(crate) style: ViewStyle,
    pub(crate) props: ViewProps,
    pub(crate) events: EventMask,
    pub(crate) text: Option<(TextLayout, Bounds<Points>)>,
    pub(crate) component: Option<Box<dyn NativeComponent>>,
    pub(crate) component_name: Option<String>,
}

impl NshView {
    pub(crate) fn new(kind: ViewKind) -> Self {
        Self {
            kind,
            parent: None,
            children: Vec::new(),
            frame: Default::default(),
            style: Default::default(),
            props: Default::default(),
            events: Default::default(),
            text: None,
            component: None,
            component_name: None,
        }
    }
}
