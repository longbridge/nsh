#![cfg(feature = "js")]

use nsh_platform_tui::{Bundle, MouseKind};
use ratatui_core::{backend::TestBackend, style::Color, terminal::Terminal};

use super::app::{draw_until, mouse, surface};

#[test]
fn hover_styles_and_events_follow_the_pointer_and_clear_outside_the_surface() {
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r##"
        import { mount, signal, View, Text } from "nsh";
        const state = signal("idle");
        mount(() => <View>
            <View style={{ width: 80, height: 32, backgroundColor: "#123456" }}
                hoverStyle={{ backgroundColor: "#abcdef" }}
                onHoverIn={() => state.set("inside")}
                onHoverOut={() => state.set("outside")}><Text>Hover</Text></View>
            <Text>{state()}</Text>
        </View>);
    "##,
        &[],
    )
    .unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "idle");
    view.handle(mouse(MouseKind::Move, 1, 0));
    draw_until(&mut view, &mut terminal, &rx, "inside");
    assert_eq!(
        terminal.backend().buffer()[(1, 1)].bg,
        Color::Rgb(0xab, 0xcd, 0xef)
    );
    view.handle(mouse(MouseKind::Move, 41, 0));
    draw_until(&mut view, &mut terminal, &rx, "outside");
    assert_eq!(
        terminal.backend().buffer()[(1, 1)].bg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
}

#[test]
fn a_responder_keeps_its_drag_outside_the_surface_and_receives_release() {
    use nsh_platform_tui::MouseButton::Left;
    let code = nsh_bundler::compile_module("App.tsx", r#"
        import { mount, signal, PanResponder, View, Text } from "nsh";
        const status = signal("ready");
        const responder = PanResponder.create({
            onStartShouldSetPanResponder: () => true,
            onPanResponderMove: (_, g) => status.set(`move ${g.dx},${g.dy}`),
            onPanResponderRelease: (_, g) => status.set(`end ${g.dx},${g.dy}`),
            onPanResponderTerminate: () => status.set("cancelled"),
        });
        mount(() => <View>
            <View {...responder.panHandlers} style={{ width: 80, height: 32 }}><Text>Drag</Text></View>
            <Text>{status()}</Text>
        </View>);
    "#, &[]).unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    view.handle(mouse(MouseKind::Move, 42, 1));
    draw_until(&mut view, &mut terminal, &rx, "move 328,16");
    view.handle(mouse(MouseKind::Up(Left), 42, 1));
    draw_until(&mut view, &mut terminal, &rx, "end 328,16");
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    view.set_focused(false);
    draw_until(&mut view, &mut terminal, &rx, "cancelled");
}

#[test]
fn a_native_pan_reports_values_and_a_drag_does_not_press() {
    use nsh_platform_tui::MouseButton::Left;
    let code = nsh_bundler::compile_module("App.tsx", r#"
        import { mount, signal, createAnimatedValue, View, Text } from "nsh";
        const x = createAnimatedValue(10), result = signal("ready"), presses = signal(0);
        mount(() => <View>
            <View pan={{x}} style={{ width: 80, height: 32 }}
                onPress={() => presses.set(presses()+1)}
                onPanStart={() => result.set("panning")}
                onPanEnd={e => result.set(`pan ${e.translationX}:${e.valueX}`)}><Text>Pan</Text></View>
            <Text>{result()} presses:{presses()}</Text>
        </View>);
    "#, &[]).unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready presses:0");
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    view.handle(mouse(MouseKind::Move, 5, 0));
    draw_until(&mut view, &mut terminal, &rx, "panning presses:0");
    view.handle(mouse(MouseKind::Up(Left), 5, 0));
    draw_until(&mut view, &mut terminal, &rx, "pan 32:42 presses:0");
}

#[test]
fn dragging_scroll_content_reveals_later_rows_without_pressing_them() {
    use nsh_platform_tui::MouseButton::Left;
    let code = nsh_bundler::compile_module("App.tsx", r#"
        import { mount, signal, Scroll, View, Text } from "nsh";
        const offset = signal(0), presses = signal(0);
        mount(() => <View>
            <Scroll style={{ width: 160, height: 48 }} onScroll={e => offset.set(e.y)}>
                <View onPress={() => presses.set(presses()+1)}>
                    <Text style={{ height: 16 }}>first</Text><Text style={{ height: 16 }}>second</Text>
                    <Text style={{ height: 16 }}>third</Text><Text style={{ height: 16 }}>fourth</Text>
                    <Text style={{ height: 16 }}>fifth</Text>
                </View>
            </Scroll>
            <Text>offset:{offset()} presses:{presses()}</Text>
        </View>);
    "#, &[]).unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "offset:0 presses:0");
    view.handle(mouse(MouseKind::Down(Left), 1, 2));
    view.handle(mouse(MouseKind::Move, 1, 0));
    view.handle(mouse(MouseKind::Up(Left), 1, 0));
    draw_until(&mut view, &mut terminal, &rx, "offset:32 presses:0");
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "t");
}

#[test]
fn a_selection_area_copies_across_texts_and_keeps_wide_graphemes_whole() {
    use nsh_core::clipboard::{ClipboardBackend, MemoryClipboard};
    use nsh_platform_tui::{AppOptions, AppView, Input, Modifiers, MouseButton::Left};
    use ratatui_core::style::Modifier;
    use std::sync::Arc;
    let clipboard = Arc::new(MemoryClipboard::default());
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.clipboard = Some(clipboard.clone());
    options.cache = nsh_platform_tui::CacheOptions::off();
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    let mut view = AppView::new(options);
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r#"
        import { mount, View, Text } from "nsh";
        mount(() => <View selectable>
            <Text>A界🙂B</Text>
            <Text style={{ userSelect: "none" }}>skip</Text>
            <Text>last</Text>
        </View>);
    "#,
        &[],
    )
    .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "last");
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    view.handle(mouse(MouseKind::Move, 2, 2));
    view.handle(mouse(MouseKind::Up(Left), 2, 2));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(
        terminal.backend().buffer()[(1, 0)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        !terminal.backend().buffer()[(0, 1)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    view.handle(Input::Key {
        key: "c".into(),
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        release: false,
    });
    assert_eq!(clipboard.text().as_deref(), Some("界🙂B\nla"));
    view.set_focused(false);
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(
        !terminal.backend().buffer()[(1, 0)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}

#[test]
fn dragging_clears_active_styles() {
    use nsh_platform_tui::MouseButton::Left;
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r##"
        import { mount, View, Text } from "nsh";
        mount(() => <View><View style={{width: 80, height: 32, backgroundColor: "#123456"}}
            activeStyle={{backgroundColor: "#abcdef"}}><Text>Hold</Text></View></View>);
    "##,
        &[],
    )
    .unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "Hold");
    let until_color =
        |view: &mut nsh_platform_tui::AppView, terminal: &mut Terminal<TestBackend>, color| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                terminal
                    .draw(|frame| view.render(frame, frame.area()))
                    .unwrap();
                if terminal.backend().buffer()[(1, 1)].bg == color {
                    break;
                }
                rx.recv_deadline(deadline).unwrap();
            }
        };
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    until_color(&mut view, &mut terminal, Color::Rgb(0xab, 0xcd, 0xef));
    view.handle(mouse(MouseKind::Move, 4, 0));
    until_color(&mut view, &mut terminal, Color::Rgb(0x12, 0x34, 0x56));
    view.handle(mouse(MouseKind::Up(Left), 4, 0));
}

#[test]
fn selectable_text_still_presses_on_a_click_and_only_copies_after_a_drag() {
    use nsh_core::clipboard::{ClipboardBackend, MemoryClipboard};
    use nsh_platform_tui::{AppOptions, AppView, Input, Modifiers, MouseButton::Left};
    use std::sync::Arc;
    let clipboard = Arc::new(MemoryClipboard::default());
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.clipboard = Some(clipboard.clone());
    options.cache = nsh_platform_tui::CacheOptions::off();
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    let mut view = AppView::new(options);
    let code = nsh_bundler::compile_module("App.tsx", r#"
        import { mount, signal, View, Text } from "nsh";
        const presses = signal(0);
        mount(() => <View>
            <Text style={{userSelect: "text"}} onPress={() => presses.set(presses()+1)}>selectable</Text>
            <Text>presses:{presses()}</Text>
        </View>);
    "#, &[]).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "presses:0");
    view.handle(mouse(MouseKind::Down(Left), 0, 0));
    view.handle(mouse(MouseKind::Up(Left), 0, 0));
    draw_until(&mut view, &mut terminal, &rx, "presses:1");
    view.handle(mouse(MouseKind::Down(Left), 0, 0));
    view.handle(mouse(MouseKind::Move, 6, 0));
    view.handle(mouse(MouseKind::Up(Left), 6, 0));
    view.handle(Input::Key {
        key: "c".into(),
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        release: false,
    });
    assert_eq!(clipboard.text().as_deref(), Some("select"));
    draw_until(&mut view, &mut terminal, &rx, "presses:1");
}

#[test]
fn concealing_a_hovered_view_clears_hover_without_more_mouse_motion() {
    use nsh_platform_tui::MouseButton::Left;
    let code = nsh_bundler::compile_module(
        "App.tsx",
        r#"
        import { mount, signal, View, Text } from "nsh";
        const visible = signal(true), state = signal("ready");
        mount(() => <View>
            <View style={{width: 80, height: 32, opacity: visible() ? 1 : 0}}
                onHoverIn={() => state.set("inside")} onHoverOut={() => state.set("left")}
                onPress={() => visible.set(false)}><Text>Hide</Text></View>
            <Text>{state()}</Text>
        </View>);
    "#,
        &[],
    )
    .unwrap();
    let (mut view, rx) = surface();
    let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    view.load(Bundle::source(code)).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "ready");
    view.handle(mouse(MouseKind::Move, 1, 0));
    draw_until(&mut view, &mut terminal, &rx, "inside");
    view.handle(mouse(MouseKind::Down(Left), 1, 0));
    view.handle(mouse(MouseKind::Up(Left), 1, 0));
    draw_until(&mut view, &mut terminal, &rx, "left");
    assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), " ");
}
