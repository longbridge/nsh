//! `host.watchlist`: GPUI Kit buttons inside the NSH app, as a native
//! component. The app gives it the desk's symbol (`symbol`) and hears the
//! one picked (`onSelect`, `{ symbol }`, by a click or Left / Right /
//! Home / End while focused), which it hands to the host
//! (`Trading.select`); the host's desk then selects it everywhere. While
//! the layers show it is framed as a GPUI widget ([`crate::layers`]).

use gpui_kit::{
    AnyElement, App, FocusHandle, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, Window,
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
    },
};
use nsh_platform_gpui::{ComponentContext, NativeComponent};
use serde_json::{Value, json};

use crate::trading::SYMBOLS;

/// The registered name, which JS declares with `nativeComponent`.
pub(crate) const NAME: &str = "host.watchlist";

/// One watchlist.
pub(crate) struct Watchlist {
    context: ComponentContext,
    symbol: String,
    focus: Option<FocusHandle>,
    /// The width last reported to nsh.
    reported: f32,
}

/// The buttons' row, with its focus ring (the app's style gives the
/// height).
const ROW: f32 = 190.0;

impl Watchlist {
    /// The component's factory (`AppView::register_component`).
    pub(crate) fn make(context: ComponentContext) -> Box<dyn NativeComponent> {
        Box::new(Self {
            context,
            symbol: "AAPL".into(),
            focus: None,
            reported: 0.0,
        })
    }
}

impl NativeComponent for Watchlist {
    fn focus_handle(&self) -> Option<FocusHandle> {
        self.focus.clone()
    }

    fn set_prop(&mut self, name: &str, value: Option<&Value>, _: &mut Window, _: &mut App) {
        if name == "symbol" {
            self.symbol = value.and_then(Value::as_str).unwrap_or("AAPL").to_owned();
        }
    }

    fn render(&mut self, _: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let width = if crate::layers::shown(cx) {
            ROW + crate::layers::WIDGET_TAG + 2.0 * crate::layers::WIDGET_BORDER
        } else {
            ROW
        };
        if width != self.reported {
            self.reported = width;
            self.context.set_size(width, 32.);
        }
        let focus = self.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let click_focus = focus.clone();
        let context = self.context.clone();
        let selected = self.symbol.clone();
        let ring = cx.theme().ring;
        let mut row = h_flex()
            .id("watchlist")
            .track_focus(&focus)
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .rounded(cx.theme().radius)
            .focus(move |style| style.border_color(ring))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&click_focus, cx)
            })
            .on_key_down(move |event, _, _| {
                let symbols = SYMBOLS.map(|(s, _)| s);
                let current = symbols
                    .iter()
                    .position(|symbol| *symbol == selected)
                    .unwrap_or(0);
                let next = match event.keystroke.key.as_str() {
                    "left" => (current + symbols.len() - 1) % symbols.len(),
                    "right" => (current + 1) % symbols.len(),
                    "home" => 0,
                    "end" => symbols.len() - 1,
                    _ => return,
                };
                context.emit("select", json!({ "symbol": symbols[next] }));
            })
            .size_full()
            .gap_2()
            .text_color(cx.theme().foreground);
        for (symbol, _) in SYMBOLS {
            let context = self.context.clone();
            let button = Button::new(symbol)
                .small()
                .tab_stop(false)
                .label(symbol)
                .on_click(move |_, _, _| context.emit("select", json!({ "symbol": symbol })));
            row = row.child(if self.symbol == symbol {
                button.primary()
            } else {
                button.outline()
            });
        }
        Some(crate::layers::widget_frame(row, cx).into_any_element())
    }
}
