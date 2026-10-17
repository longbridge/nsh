//! What the JS side is decoded with and typed from, so the two cannot
//! drift apart: the animation graph's codes ([`protocol`]), the canvas
//! operations and their decoding ([`canvas`], with [`ops`]'
//! `#[derive(JsOps)]`), the style types ([`style`]), the host elements'
//! props ([`host`]: their decoding and types, from `#[derive(HostProps)]`
//! on [`crate::Prop`]) and the easings and layout animations ([`animation`]).
//! The decoders in `nsh-js` match on the same constants and call the
//! derived decoders, and their declarations go into the ones apps are
//! checked against (`nsh-js` `declarations.rs`).

use std::time::Duration;

/// nsh's own native ES modules, which bundles compiled with nsh's JSX
/// compiler import (served by `nsh-js`, left as imports by the bundler):
/// `nsh`, the API apps write against; `nsh:rt`, the renderer compiled JSX
/// calls; `nsh/device` and `nsh/accessibility`, the environment; the
/// platforms' and host elements' modules. Registries add theirs
/// (`Registry::specifier`: `nsh/ui/mobile`, ...) or the functions of
/// nsh's own (`nsh/image`, `nsh/android`).
pub const NATIVE_MODULES: &[&str] = &[
    "nsh",
    "nsh:rt",
    "nsh/device",
    "nsh/accessibility",
    "nsh/android",
    "nsh/ios",
    "nsh/desktop",
    "nsh/webview",
    "nsh/video",
    "nsh/native",
    "nsh/gesture",
    "nsh/animated",
    "nsh/canvas",
    "nsh/list",
    "nsh/image",
    "nsh/keymap",
];

/// The module name of an app's bundle (its entry chunk): stack traces show
/// `bundle.js:LINE:COL`, and the chunks of a split bundle import it as
/// `./bundle.js`.
pub const ENTRY_CHUNK: &str = "bundle.js";

/// The global function a split bundle's dynamic imports call (the bundler
/// rewrites `import("./Screen.js")` into `__nsh_import("Screen.js",
/// ["shared.js"])`): it makes the chunk and the chunks it imports
/// statically available (on the web, fetched), then imports the first.
pub const IMPORT_CHUNK: &str = "__nsh_import";

/// Resource limits of one VM.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// QuickJS heap, in bytes (0: no limit). Running out stops the VM.
    pub memory_bytes: usize,
    /// Interpreter stack, in bytes: deep recursion becomes a `RangeError`
    /// rather than a native stack overflow. Must stay below the engine
    /// thread's stack.
    pub stack_bytes: usize,
    /// Evaluating the bundle.
    pub load_budget: Duration,
    /// Any other call into JS: one event (with the rendering it causes), one
    /// list bind, one timer, one frame's callbacks, the microtask drain.
    pub call_budget: Duration,
    /// Changes JS may make at once (the op queue's length): they are held in
    /// Rust memory, which the heap limit does not cover.
    pub max_op_values: usize,
    /// Operations one canvas drawing may record.
    pub max_canvas_ops: usize,
    /// Nodes the core keeps (each may have a native view, whose memory the
    /// heap limit does not cover either).
    pub max_nodes: usize,
}

impl Limits {
    /// The limits of an app nsh runs on its own on a desktop (the macOS,
    /// Windows and Linux hosts and runners): a 1 GB heap, as a desktop app
    /// may hold much more data than a module embedded in a host app.
    pub fn standalone_desktop() -> Self {
        Self {
            memory_bytes: 1 << 30,
            ..Self::default()
        }
    }

    /// The limits of an app nsh runs on its own on a phone or tablet (the
    /// iOS and Android hosts and runners): a 512 MB heap, well below where
    /// the systems end apps for their memory.
    pub fn standalone_mobile() -> Self {
        Self {
            memory_bytes: 512 << 20,
            ..Self::default()
        }
    }
}

/// The limits of an app embedded in a host app (`NshAppView`, `NshView`,
/// `AppView`): a 256 MB heap; the embedder may set others.
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 256 << 20,
            stack_bytes: 4 << 20,
            load_budget: Duration::from_secs(10),
            call_budget: Duration::from_secs(2),
            max_op_values: 8_000_000,
            max_canvas_ops: 1_000_000,
            max_nodes: 200_000,
        }
    }
}

/// Constants of encoded operations and a table of them for the generators.
macro_rules! codes {
    ($table:ident: $($(#[doc = $doc:literal])* $name:ident = $value:literal),* $(,)?) => {
        $(
            #[doc = concat!("`", stringify!($name), "`.")]
            $(#[doc = $doc])*
            pub const $name: i32 = $value;
        )*

        /// The codes by name, with their documentation (the operands).
        pub const $table: &[(&str, i32, &str)] = &[$((stringify!($name), $value, concat!($($doc),*))),*];
    };
}

pub mod animation;
pub mod canvas;
pub mod host;
pub(crate) mod host_values;
pub mod ops;
pub mod protocol;
pub mod style;

#[cfg(test)]
mod tests {
    #[test]
    fn the_declarations_match_the_tables() {
        let canvas = super::canvas::declarations();
        assert!(
            canvas.contains("  moveTo(x: CanvasNumber, y: CanvasNumber): void;"),
            "{canvas}"
        );
        assert!(canvas.contains("counterclockwise?: boolean): void;"));
        assert!(canvas.contains("  lineCap: LineCap;"));
        assert!(canvas.contains("export type LineCap = \"butt\" | \"round\" | \"square\";"));
        let style = super::style::declarations();
        // Every property the core parses has a type, in its interface.
        for (name, ts) in [
            ("boxShadow", "string"),
            ("gridTemplateColumns", "string"),
            ("transform", "TransformStep[]"),
            ("tintColor", "ColorValue"),
            ("display", "\"flex\" | \"grid\" | \"block\" | \"none\""),
        ] {
            assert!(style.contains(&format!("  {name}?: {ts};")), "{name}");
        }
        let text = style.find("export interface TextStyle").unwrap();
        assert!(style[text..].contains("  textDecorationStyle?:"));
        // Each host prop and event in the interface of the elements it is for.
        let elements = super::host::declarations();
        for (interface, prop) in [
            ("HostProps", "  pointerEvents?: PointerEvents;"),
            ("HostProps", "  onPress?: () => void;"),
            ("HostProps", "  onLayout?: (e: LayoutEvent) => void;"),
            ("AndroidHostProps", "  elevation?: number;"),
            ("InputHostProps", "  keyboardType?: KeyboardType;"),
            ("InputHostProps", "  onChangeText?: (text: string) => void;"),
            ("IOSInputHostProps", "  inputAccessoryViewID?: string;"),
            (
                "IOSInputHostProps",
                "  keyboardAppearance?: KeyboardAppearance;",
            ),
            ("TextHostProps", "  selectionColor?: ColorValue;"),
            ("InputHostProps", "  selectionColor?: ColorValue;"),
            (
                "ImageHostProps",
                "  onLoad?: (e: { width: number; height: number }) => void;",
            ),
            (
                "VideoHostProps",
                "  onLoad?: (e: { duration: number; width: number; height: number }) => void;",
            ),
            (
                "ScrollingHostProps",
                "  decelerationRate?: \"normal\" | \"fast\" | number;",
            ),
            (
                "ScrollingHostProps",
                "  onScroll?: (e: ScrollEvent) => void;",
            ),
            (
                "ListHostProps",
                "  contentContainerStyle?: ListContentStyle;",
            ),
            (
                "IOSModalHostProps",
                "  presentationStyle?: ModalPresentation;",
            ),
            (
                "DesktopModalHostProps",
                "  presentationStyle?: ModalPresentation;",
            ),
            (
                "WebModalHostProps",
                "  presentationStyle?: ModalPresentation;",
            ),
            ("DesktopScreenHostProps", "  headerBackTitle?: string;"),
            (
                "DesktopWebViewHostProps",
                "  allowsBackForwardNavigationGestures?: boolean;",
            ),
            ("ModalHostProps", "  onRequestClose?: () => void;"),
        ] {
            let at = elements
                .find(&format!("export interface {interface} {{"))
                .unwrap_or_else(|| panic!("{interface}"));
            let end = at + elements[at..].find("\n}").unwrap();
            assert!(elements[at..end].contains(prop), "{interface}: {prop}");
        }
        assert!(elements.contains(
            "export type KeyboardType = \"default\" | \"number-pad\" | \"decimal-pad\" | \"numeric\" | \"numbers-and-punctuation\""
        ));
        assert!(
            elements
                .contains("export type KeyboardAppearance = \"default\" | \"light\" | \"dark\";")
        );
        let animation = super::animation::declarations();
        assert!(animation.contains("  back(s?: number): EasingCurve;"));
        assert!(
            animation.contains("export type LayoutAnimationType = \"easeInEaseOut\" | \"spring\"")
        );
        assert!(animation.contains("  readonly scaleXY: \"scaleXY\";"));
        // What the runtime sets itself is not part of the types.
        for internal in ["splice?", "cellIndex?", "tailwindTheme?", "headerSlot?"] {
            assert!(!elements.contains(internal), "{internal}");
        }
    }

    #[test]
    fn every_event_is_typed_or_internal() {
        use super::host::{EVENTS, INTERNAL_EVENTS};
        for kind in crate::EventKind::ALL {
            let typed = EVENTS.iter().any(|e| e.0 == *kind);
            let internal = INTERNAL_EVENTS.contains(kind);
            assert!(
                typed != internal,
                "{}: give it a handler type in `EVENTS` or list it in `INTERNAL_EVENTS`",
                kind.prop_name()
            );
        }
    }
}
