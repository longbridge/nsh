use std::time::{Duration, Instant};

use nsh_platform_tui::{AppView, Bundle, Input, MouseButton, MouseKind};
use ratatui_core::{backend::TestBackend, buffer::Buffer, style::Color, terminal::Terminal};

use super::app::{mouse, surface};

fn load(
    source: &str,
) -> (
    AppView,
    crossbeam_channel::Receiver<()>,
    Terminal<TestBackend>,
) {
    let code = nsh_bundler::compile_module("Animation.tsx", source, &[]).unwrap();
    let (mut view, wake) = surface();
    view.on_error(|error| panic!("{error}"));
    view.load(Bundle::source(code)).unwrap();
    (view, wake, Terminal::new(TestBackend::new(40, 12)).unwrap())
}

fn render_until(
    view: &mut AppView,
    terminal: &mut Terminal<TestBackend>,
    wake: &crossbeam_channel::Receiver<()>,
    mut ready: impl FnMut(&Buffer, &AppView) -> bool,
) {
    let timeout = Instant::now() + Duration::from_secs(5);
    loop {
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .unwrap();
        if ready(terminal.backend().buffer(), view) {
            return;
        }
        assert!(
            Instant::now() < timeout,
            "animation did not reach its visible result: {:?}",
            contents(terminal.backend().buffer())
        );
        let deadline = view.next_frame_deadline().unwrap_or(timeout).min(timeout);
        let _ = wake.recv_deadline(deadline);
    }
}

fn contents(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

fn press(view: &mut AppView, key: &str) {
    view.handle(Input::Key {
        key: key.into(),
        modifiers: Default::default(),
        release: false,
    });
}

#[test]
fn animated_opacity_finishes_once_and_the_surface_returns_to_idle() {
    let (mut view, wake, mut terminal) = load(
        r#"
        import { mount, View, Text, signal, createAnimatedValue, Animated } from "nsh";
        mount(() => {
            const opacity = createAnimatedValue(0);
            const done = signal(0);
            return <View>
                <View onPress={() => Animated.sequence([Animated.delay(40), Animated.timing(opacity, { toValue: 1, duration: 80 })])
                    .start(({ finished }) => { if (finished) done.set(done() + 1); })}>
                    <Text>Run</Text>
                </View>
                <Text style={{ opacity }}>Visible</Text>
                <Text>Done {done()}</Text>
            </View>;
        });
    "#,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Done 0")
    });
    assert!(!contents(terminal.backend().buffer()).contains("Visible"));
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        let text = contents(buffer);
        text.contains("Done 1") && text.contains("Visible") && view.next_frame_deadline().is_none()
    });
    for _ in wake.try_iter() {}
    assert!(wake.recv_timeout(Duration::from_millis(50)).is_err());
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(contents(terminal.backend().buffer()).contains("Done 1"));
    assert!(view.next_frame_deadline().is_none());
}

#[test]
fn bound_colors_override_styles_and_unbinding_restores_them() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, signal, createAnimatedValue } from "nsh";
        import { Input } from "nsh/base";
        mount(() => {
            const bound = signal(true);
            const value = createAnimatedValue(1);
            const color = value.interpolate({ inputRange: [0, 1], outputRange: ["#ff0000", "#00ff00"] });
            return <View>
                <View onPress={() => bound.set(false)}><Text>Unbind</Text></View>
                <View style={{ width: 80, height: 48, borderWidth: 8, backgroundColor: bound() ? color : "#ff0000",
                    borderColor: bound() ? color : "#ff0000" }} />
                <Text style={{ color: bound() ? color : "#ff0000" }}>Color</Text>
                <Input value="Field" style={{ width: 80, height: 16, color: bound() ? color : "#ff0000" }} />
            </View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Field")
    });
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].fg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(1, 2)].bg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(0, 4)].fg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(0, 5)].fg, Color::Rgb(0, 255, 0));
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        buffer[(0, 4)].fg == Color::Rgb(255, 0, 0)
    });
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].fg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(1, 2)].bg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(0, 5)].fg, Color::Rgb(255, 0, 0));
}

#[test]
fn animated_transforms_move_visible_cells_and_their_press_targets() {
    let (mut view, wake, mut terminal) = load(
        r#"
        import { mount, View, Text, signal, createAnimatedValue } from "nsh";
        mount(() => {
            const x = createAnimatedValue(0);
            const presses = signal(0);
            return <View>
                <View onPress={() => x.set(16)}><Text>Move</Text></View>
                <View style={{ width: 80, height: 32, overflow: "hidden" }}>
                    <View onPress={() => presses.set(presses() + 1)}
                        style={{ width: 32, height: 16, transform: [{ translateX: x }] }}>
                        <Text>Tile</Text>
                    </View>
                </View>
                <Text>Presses {presses()}</Text>
            </View>;
        });
    "#,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Presses 0")
    });
    assert_eq!(terminal.backend().buffer()[(0, 1)].symbol(), "T");
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        buffer[(2, 1)].symbol() == "T"
    });
    assert_eq!(terminal.backend().buffer()[(0, 1)].symbol(), " ");
    for kind in [
        MouseKind::Down(MouseButton::Left),
        MouseKind::Up(MouseButton::Left),
    ] {
        view.handle(mouse(kind, 2, 1));
    }
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Presses 1")
    });
}

#[test]
fn scale_and_rotation_project_descendants_around_the_transform_origin() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, createAnimatedValue } from "nsh";
        mount(() => {
            const scale = createAnimatedValue(2);
            const angle = createAnimatedValue(90);
            return <View>
                <View style={{ position: "absolute", left: 16, top: 16, width: 32, height: 32,
                    transformOrigin: "top left", scale }}>
                    <View style={{ position: "absolute", left: 8, top: 0, width: 8, height: 16, backgroundColor: "#ff0000" }} />
                </View>
                <View style={{ position: "absolute", left: 80, top: 16, width: 32, height: 32,
                    transformOrigin: "top left", rotate: angle }}>
                    <View style={{ position: "absolute", left: 16, top: 0, width: 8, height: 16, backgroundColor: "#00ff00" }} />
                </View>
                <Text>Transformed</Text>
            </View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Transformed")
    });
    let buffer = terminal.backend().buffer();
    for y in 1..3 {
        for x in 4..6 {
            assert_eq!(buffer[(x, y)].bg, Color::Rgb(255, 0, 0));
        }
    }
    assert_eq!(buffer[(3, 1)].bg, Color::Reset);
    assert_eq!(buffer[(8, 2)].bg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(9, 2)].bg, Color::Rgb(0, 255, 0));
    assert_eq!(buffer[(10, 2)].bg, Color::Reset);
}

#[test]
fn canvas_slots_follow_native_values_and_appear_effects_end_visible() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, Canvas, createAnimatedValue, signal } from "nsh";
        mount(() => {
            const value = createAnimatedValue(0);
            const done = signal(false);
            const x = value.interpolate({ inputRange: [0, 1], outputRange: [0, 16] });
            const color = value.interpolate({ inputRange: [0, 1], outputRange: ["#ff0000", "#00ff00"] });
            value.timing(1, { duration: 80 }).then(() => done.set(true));
            return <View>
                <View style={{ opacity: value, translate: [x, 0] }}><Text>Appeared</Text></View>
                <Canvas style={{ width: 80, height: 16 }} draw={cx => {
                    cx.fillStyle = color;
                    cx.translate(x, 0);
                    cx.fillRect(0, 0, 16, 16);
                }} />
                <Text>{done() ? "Finished" : "Running"}</Text>
            </View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        contents(buffer).contains("Finished") && view.next_frame_deadline().is_none()
    });
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(2, 0)].symbol(), "A");
    assert_eq!(buffer[(0, 1)].symbol(), " ");
    assert_eq!(buffer[(2, 1)].symbol(), "⣿");
    assert_eq!(buffer[(3, 1)].symbol(), "⣿");
    assert_eq!(buffer[(2, 1)].fg, Color::Rgb(0, 255, 0));
}

#[test]
fn animated_page_backgrounds_still_use_the_terminals_background() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, createAnimatedValue } from "nsh";
        mount(() => {
            const value = createAnimatedValue(0);
            const color = value.interpolate({ inputRange: [0, 1], outputRange: ["#ff0000", "#00ff00"] });
            return <View style={{ backgroundColor: color }}>
                <View onPress={() => value.set(1)}><Text>Change page</Text></View>
                <View style={{ width: 80, height: 16, backgroundColor: "#00ff00" }} />
            </View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Change page")
    });
    assert_eq!(terminal.backend().buffer()[(0, 0)].bg, Color::Reset);
    assert_eq!(
        terminal.backend().buffer()[(0, 1)].bg,
        Color::Rgb(0, 255, 0)
    );
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        buffer[(0, 1)].bg == Color::Reset
    });
    assert_eq!(terminal.backend().buffer()[(0, 0)].bg, Color::Reset);
}

#[test]
fn native_frames_advance_values_and_stopping_reports_cancellation_then_idles() {
    let (mut view, wake, mut terminal) = load(
        r#"
        import { mount, View, Text, signal, createAnimatedValue, Animated, Easing } from "nsh";
        mount(() => {
            const x = createAnimatedValue(0);
            const result = signal("Ready");
            const animation = Animated.timing(x, { toValue: 240, duration: 10000, easing: Easing.linear });
            return <View>
                <View onPress={() => animation.start(({ finished }) => result.set(finished ? "Completed" : "Stopped"))}><Text>Run</Text></View>
                <View onPress={() => animation.stop()}><Text>Stop</Text></View>
                <Text style={{ translate: [x, 0] }}>Moving</Text>
                <Text>{result()}</Text>
            </View>;
        });
    "#,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Ready")
    });
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        (1..30).any(|x| buffer[(x, 2)].symbol() == "M") && view.next_frame_deadline().is_some()
    });
    press(&mut view, "Tab");
    press(&mut view, "Enter");
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        contents(buffer).contains("Stopped") && view.next_frame_deadline().is_none()
    });
    let stopped = terminal.backend().buffer().clone();
    for _ in wake.try_iter() {}
    assert!(wake.recv_timeout(Duration::from_millis(50)).is_err());
    terminal
        .draw(|frame| frame.render_widget(&mut view, frame.area()))
        .unwrap();
    assert_eq!(terminal.backend().buffer(), &stopped);
    assert!(view.next_frame_deadline().is_none());
}

#[test]
fn replacing_an_animating_app_discards_its_graph_and_deadline() {
    let (mut view, wake, mut terminal) = load(
        r#"
        import { mount, View, Text, createAnimatedValue, Animated } from "nsh";
        mount(() => {
            const value = createAnimatedValue(0);
            Animated.loop(Animated.timing(value, { toValue: 1, duration: 1000 })).start();
            return <View><Text>Loop</Text><Text style={{ opacity: value }}>Pulse</Text></View>;
        });
    "#,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        contents(buffer).contains("Loop") && view.next_frame_deadline().is_some()
    });
    let code = nsh_bundler::compile_module(
        "Replacement.tsx",
        r#"
        import { mount, Text } from "nsh";
        mount(() => <Text>Replacement</Text>);
    "#,
        &[],
    )
    .unwrap();
    view.load(Bundle::source(code)).unwrap();
    render_until(&mut view, &mut terminal, &wake, |buffer, view| {
        contents(buffer).contains("Replacement") && view.next_frame_deadline().is_none()
    });
    assert!(!contents(terminal.backend().buffer()).contains("Loop"));
    view.stop();
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(view.next_frame_deadline().is_none());
}

#[test]
fn canvas_reset_transform_preserves_the_views_animated_transform() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, Canvas, createAnimatedValue } from "nsh";
        mount(() => {
            const scale = createAnimatedValue(2);
            return <View>
                <Text>Canvas transform</Text>
                <Canvas style={{ position: "absolute", left: 16, top: 16, width: 16, height: 16,
                    transformOrigin: "top left", scale }} draw={cx => {
                    cx.translate(80, 0);
                    cx.setTransform(1, 0, 0, 1, 0, 0);
                    cx.fillStyle = "#ff0000";
                    cx.fillRect(0, 0, 8, 16);
                }} />
            </View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Canvas transform")
    });
    let buffer = terminal.backend().buffer();
    for y in 1..3 {
        for x in 2..4 {
            assert_eq!(buffer[(x, y)].symbol(), "⣿");
            assert_eq!(buffer[(x, y)].fg, Color::Rgb(255, 0, 0));
        }
    }
    assert_eq!(buffer[(4, 1)].symbol(), " ");
}

#[test]
fn animated_parent_opacity_blends_descendant_text_in_cells() {
    let (mut view, wake, mut terminal) = load(
        r##"
        import { mount, View, Text, createAnimatedValue } from "nsh";
        mount(() => {
            const opacity = createAnimatedValue(0.5);
            return <View><View style={{ opacity }}><Text style={{ color: "#ffffff" }}>Faded</Text></View></View>;
        });
    "##,
    );
    render_until(&mut view, &mut terminal, &wake, |buffer, _| {
        contents(buffer).contains("Faded")
    });
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].fg,
        Color::Rgb(128, 128, 128)
    );
}
