use super::*;
use crate::{
    effects::{BlendMode, Cursor, GradientKind, Keyframes},
    style::{Color, Style},
    transform::TransformKind,
};

fn cx(states: u8) -> (ClassEnvironment, u8) {
    (ClassEnvironment::default(), states)
}

/// Applies `classes` with the given environment and states.
fn styled_with(
    classes: &str,
    env: ClassEnvironment,
    window: (f32, f32),
    states: u8,
) -> (Style, ClassProps) {
    let theme = Theme::default();
    let list = parse(&theme, classes);
    assert!(
        list.unknown().is_empty(),
        "unknown classes: {:?}",
        list.unknown()
    );
    let mut style = Style::default();
    let mut props = ClassProps::default();
    let group = |_: &str| 0;
    let context = Context {
        env,
        window,
        rtl: false,
        states,
        position: 0,
        group: &group,
    };
    list.apply(&context, &theme, &mut style, &mut props, |_| {});
    (style, props)
}

fn styled(classes: &str) -> Style {
    let (env, states) = cx(0);
    styled_with(classes, env, (390.0, 844.0), states).0
}

#[test]
fn spacing_and_layout() {
    let s = styled(
        "flex flex-row items-center justify-between gap-3 p-4 px-2 -mt-1 w-1/2 h-12 min-w-0",
    );
    assert_eq!(s.layout.flex_direction, taffy::FlexDirection::Row);
    assert_eq!(s.layout.align_items, taffy::AlignItems::CENTER);
    assert_eq!(s.layout.justify_content, taffy::AlignContent::SPACE_BETWEEN);
    assert_eq!(s.layout.gap.width, taffy::LengthPercentage::length(12.0));
    assert_eq!(s.layout.padding.left, taffy::LengthPercentage::length(8.0));
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(16.0));
    assert_eq!(
        s.layout.margin.top,
        taffy::LengthPercentageAuto::length(-4.0)
    );
    assert_eq!(s.layout.size.width, taffy::Dimension::percent(0.5));
    assert_eq!(s.layout.size.height, taffy::Dimension::length(48.0));
    let s = styled("flex-[2]");
    assert_eq!(s.layout.flex_grow, 2.0);
    let s = styled("flex-[3_0_50%]");
    assert_eq!((s.layout.flex_grow, s.layout.flex_shrink), (3.0, 0.0));
    assert_eq!(s.layout.flex_basis, taffy::Dimension::percent(0.5));
    let s = styled("p-[13px] m-auto inset-0 absolute size-10 basis-1/3 flex-1");
    assert_eq!(s.layout.padding.left, taffy::LengthPercentage::length(13.0));
    assert_eq!(s.layout.margin.left, taffy::LengthPercentageAuto::auto());
    assert_eq!(s.layout.position, taffy::Position::Absolute);
    assert_eq!(
        s.layout.inset.right,
        taffy::LengthPercentageAuto::length(0.0)
    );
    assert_eq!(s.layout.size.height, taffy::Dimension::length(40.0));
    assert_eq!(s.layout.flex_grow, 1.0);
}

#[test]
fn later_classes_win() {
    let s = styled("p-4 p-2 bg-red-500 bg-blue-500");
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(8.0));
    assert_eq!(s.view.background_color, Color(0x2b7f_ffff));
    // Variants apply after plain utilities, whatever the order.
    let (s, _) = styled_with(
        "active:bg-red-500 bg-blue-500",
        ClassEnvironment::default(),
        (390.0, 844.0),
        state::PRESSED,
    );
    assert_eq!(s.view.background_color, Color(0xfb2c_36ff));
}

#[test]
fn colors_and_opacity() {
    let s = styled("bg-white/50 text-zinc-900 border border-[#ff0000] opacity-75");
    assert_eq!(s.view.background_color, Color(0xffff_ff80));
    assert_eq!(s.text.color, Some(Color(0x1818_1bff)));
    assert_eq!(s.view.borders.widths, [1.0; 4]);
    assert_eq!(s.view.borders.colors[0], Color(0xff00_00ff));
    assert_eq!(s.view.opacity, 0.75);
    let s = styled("bg-[rgb(0_0_255)]/25 text-black/[0.5]");
    assert_eq!(s.view.background_color, Color(0x0000_ff40));
    assert_eq!(s.text.color, Some(Color(0x0000_0080)));
}

#[test]
fn theme_tokens() {
    let ui = crate::ui::Theme::default();
    let dark = ClassEnvironment {
        dark: true,
        ..Default::default()
    };
    let s = styled("bg-primary text-muted-foreground border border-border rounded-(--radius)");
    assert_eq!(s.view.background_color, ui.primary.light);
    assert_eq!(s.text.color, Some(ui.muted_foreground.light));
    assert_eq!(s.view.borders.colors[0], ui.border.light);
    assert_eq!(s.view.border_radius, ui.radius);
    let (s, _) = styled_with(
        "bg-primary text-(--muted-foreground)",
        dark,
        (390.0, 844.0),
        0,
    );
    assert_eq!(s.view.background_color, ui.primary.dark);
    assert_eq!(s.text.color, Some(ui.muted_foreground.dark));
    // Opacity modifiers, longer prefixes, variants, the long form.
    let s = styled("bg-primary-background/50 border-t-2 border-t-ring");
    assert_eq!(s.view.background_color, ui.primary.light.with_alpha(0.5));
    assert_eq!(s.view.borders.colors[0], ui.ring.light);
    // Parts and props: shadows, rings, gradients, the caret, placeholders.
    let (s, props) = styled_with(
        "shadow-md shadow-primary ring-2 ring-ring bg-linear-to-r from-primary to-accent caret-primary placeholder:text-muted-foreground",
        ClassEnvironment::default(),
        (390.0, 844.0),
        0,
    );
    assert_eq!(props.caret_color, Some(ui.primary.light));
    assert_eq!(props.placeholder_color, Some(ui.muted_foreground.light));
    let e = s.view.effects.as_ref().unwrap();
    assert!(e.box_shadows.iter().any(|l| l.color == ui.ring.light));
    // The shadow's color keeps each layer's alpha.
    assert!(
        e.box_shadows
            .iter()
            .any(|l| l.color.0 >> 8 == ui.primary.light.0 >> 8)
    );
    let g = &e.background_images[0];
    assert_eq!(g.stops[0].color, ui.primary.light);
    assert_eq!(g.stops.last().unwrap().color, ui.accent.light);
    let (s, _) = styled_with("dark:bg-accent", dark, (390.0, 844.0), 0);
    assert_eq!(s.view.background_color, ui.accent.dark);
    let s = styled("text-(length:--font-size) rounded-(--radius-lg)");
    assert_eq!(s.text.font_size, Some(ui.font_size));
    assert_eq!(s.view.border_radius, ui.radius_lg);
    // Later utilities still win, in both appearances.
    let (s, _) = styled_with("bg-primary bg-red-500", dark, (390.0, 844.0), 0);
    assert_eq!(s.view.background_color, Color(0xfb2c_36ff));
    let list = parse(&Theme::default(), "bg-primary p-4");
    assert_ne!(list.deps() & deps::THEME, 0);
    assert_ne!(list.deps() & deps::DARK, 0);
    assert_eq!(
        parse(&Theme::default(), "bg-red-500").deps() & deps::THEME,
        0
    );
    // Names that are no token, and variables there are not.
    let list = parse(&Theme::default(), "bg-nope text-(--nope) w-primary");
    assert_eq!(list.unknown().len(), 3);
}

#[test]
fn tailwind_colors_win_over_tokens_and_tokens_come_from_the_theme() {
    let mut theme = Theme::from_json(r##"{ "colors": { "primary": "#ff0000" } }"##).unwrap();
    let ui = crate::ui::Theme::from_json(
        r##"{ "themes": [
            { "mode": "light", "radius.card": 10, "colors": { "chart.up": "#00ff00", "chart.grid": "#0000ff" } },
            { "mode": "dark", "colors": { "chart.up": "#008800" } }
        ] }"##,
    )
    .unwrap();
    theme.set_tokens(&ui);
    let group = |_: &str| 0;
    let apply = |classes: &str, dark: bool| {
        let list = parse(&theme, classes);
        assert!(list.unknown().is_empty(), "{:?}", list.unknown());
        let mut style = Style::default();
        let context = Context {
            env: ClassEnvironment {
                dark,
                ..Default::default()
            },
            window: (390.0, 844.0),
            rtl: false,
            states: 0,
            position: 0,
            group: &group,
        };
        list.apply(
            &context,
            &theme,
            &mut style,
            &mut ClassProps::default(),
            |_| {},
        );
        style
    };
    assert_eq!(
        apply("bg-primary", false).view.background_color,
        Color(0xff00_00ff)
    );
    assert_eq!(
        apply("bg-chart-up", false).view.background_color,
        Color(0x00ff_00ff)
    );
    assert_eq!(
        apply("bg-chart-up", true).view.background_color,
        Color(0x0088_00ff)
    );
    // A color set in one mode only is used in both.
    let s = apply("border border-chart-grid rounded-(--radius-card)", true);
    assert_eq!(s.view.borders.colors[0], Color(0x0000_ffff));
    assert_eq!(s.view.border_radius, 10.0);
}

#[test]
fn typography() {
    let s = styled(
        "text-lg font-semibold italic tracking-wide leading-tight underline decoration-wavy decoration-red-500 uppercase text-center tabular-nums",
    );
    assert_eq!(s.text.font_size, Some(18.0));
    assert_eq!(s.text.font_weight, Some(600));
    assert_eq!(s.text.italic, Some(true));
    assert_eq!(s.text.letter_spacing_em, Some(0.025));
    assert_eq!(s.text.line_height_multiple, Some(1.25));
    assert!(s.text.decoration.unwrap().underline);
    assert_eq!(
        s.text.decoration_style,
        Some(crate::style::DecorationStyle::Wavy)
    );
    assert_eq!(s.text.decoration_color, Some(Color(0xfb2c_36ff)));
    assert_eq!(s.text.text_align, Some(crate::style::TextAlign::Center));
    assert!(
        s.text
            .font_variant
            .unwrap()
            .has(crate::style::FontVariant::TABULAR_NUMS)
    );
    let s = styled("text-sm/6 font-mono");
    assert_eq!(s.text.font_size, Some(14.0));
    assert_eq!(s.text.line_height, Some(24.0));
    assert_eq!(s.text.font_family.unwrap().name().as_ref(), "monospace");
    let (_, props) = styled_with("truncate", ClassEnvironment::default(), (390.0, 844.0), 0);
    assert_eq!(props.number_of_lines, Some(1));
    assert_eq!(props.no_wrap, Some(true));
}

#[test]
fn shadows_and_rings() {
    let s = styled("shadow-md");
    let e = s.view.effects.as_ref().unwrap();
    assert_eq!(e.box_shadows.len(), 2);
    assert_eq!(e.box_shadows[0].offset_y, 4.0);
    assert_eq!(e.box_shadows[0].spread, -1.0);
    let s = styled("shadow-lg shadow-blue-500/50 ring-2 ring-offset-2 ring-red-500");
    let e = s.view.effects.as_ref().unwrap();
    // Ring offset, ring, then the two shadow layers.
    assert_eq!(e.box_shadows.len(), 4);
    assert_eq!(e.box_shadows[0].spread, 2.0);
    assert_eq!(e.box_shadows[0].color, Color(0xffff_ffff));
    assert_eq!(e.box_shadows[1].spread, 4.0);
    assert_eq!(e.box_shadows[1].color, Color(0xfb2c_36ff));
    assert_eq!(e.box_shadows[2].color, Color(0x2b7f_ff80));
    // A color without an alpha keeps each layer's.
    let s = styled("shadow-sm shadow-red-500");
    let e = s.view.effects.as_ref().unwrap();
    assert_eq!(e.box_shadows[0].color, Color(0xfb2c_361a));
    assert!(styled("shadow-md shadow-none").view.effects.is_none());
}

#[test]
fn gradients() {
    let s = styled("bg-linear-to-r from-blue-500 via-purple-500 via-40% to-pink-500");
    let g = &s.view.effects.as_ref().unwrap().background_images[0];
    assert_eq!(g.kind, GradientKind::Linear { angle: 90.0 });
    assert_eq!(g.stops.len(), 3);
    assert_eq!(g.stops[1].position, Some(0.4));
    let s = styled("bg-linear-to-br from-red-500");
    let g = &s.view.effects.as_ref().unwrap().background_images[0];
    assert!(matches!(
        g.kind,
        GradientKind::LinearCorner {
            right: true,
            bottom: true
        }
    ));
    assert_eq!(g.stops[1].color, Color::TRANSPARENT);
    let s = styled("bg-[linear-gradient(45deg,red,blue)]");
    let g = &s.view.effects.as_ref().unwrap().background_images[0];
    assert_eq!(g.kind, GradientKind::Linear { angle: 45.0 });
}

#[test]
fn state_backgrounds_show_over_gradients() {
    let classes = "bg-linear-to-b from-zinc-700 to-zinc-800 active:bg-zinc-600";
    let pressed = |classes| {
        let (env, _) = cx(0);
        styled_with(classes, env, (390.0, 844.0), state::PRESSED).0
    };
    assert!(
        !styled(classes)
            .view
            .effects
            .as_ref()
            .unwrap()
            .background_images
            .is_empty()
    );
    let s = pressed(classes);
    assert!(s.view.effects.is_none());
    assert_eq!(s.view.background_color, Color(0x52_52_5c_ff));
    // A state that sets its own gradient keeps it.
    let s = pressed("bg-linear-to-b from-zinc-700 active:bg-zinc-600 active:from-zinc-500");
    assert_eq!(s.view.effects.as_ref().unwrap().background_images.len(), 1);
    // A later plain color is not feedback.
    let env = ClassEnvironment {
        dark: true,
        ..Default::default()
    };
    let (s, _) = styled_with(
        "bg-linear-to-b from-zinc-700 active:bg-zinc-600 dark:bg-black",
        env,
        (390.0, 844.0),
        state::PRESSED,
    );
    assert_eq!(s.view.effects.as_ref().unwrap().background_images.len(), 1);
    // Plain colors keep CSS's order: the gradient is drawn over them.
    let s = styled("bg-zinc-600 bg-linear-to-b from-zinc-700");
    assert_eq!(s.view.effects.as_ref().unwrap().background_images.len(), 1);
}

#[test]
fn transforms() {
    let s = styled("-translate-x-1/2 translate-y-2 rotate-45 scale-110");
    let ops = s.view.transform.ops();
    assert_eq!(ops[0].kind, TransformKind::TranslateXFraction);
    assert_eq!(ops[0].value, -0.5);
    assert_eq!(ops[1].kind, TransformKind::TranslateY);
    assert_eq!(ops[1].value, 8.0);
    assert_eq!(ops[2].kind, TransformKind::Rotate);
    assert_eq!(ops[3].kind, TransformKind::Scale);
    assert!((ops[3].value - 1.1).abs() < 1e-6);
    let t = s.view.transform.resolve_fractions(100.0, 40.0);
    assert_eq!(t.ops()[0].kind, TransformKind::TranslateX);
    assert_eq!(t.ops()[0].value, -50.0);
}

#[test]
fn effects_and_interaction() {
    let s = styled(
        "blur-sm brightness-125 backdrop-blur-md mix-blend-multiply outline-2 outline-offset-2 outline-blue-500 invisible cursor-pointer",
    );
    let e = s.view.effects.as_ref().unwrap();
    assert_eq!(
        e.filters,
        vec![
            crate::effects::Filter::Blur(8.0),
            crate::effects::Filter::Brightness(1.25)
        ]
    );
    assert_eq!(e.backdrop_filters, vec![crate::effects::Filter::Blur(12.0)]);
    assert_eq!(s.view.blend_mode, BlendMode::Multiply);
    assert_eq!(s.view.outline.width, 2.0);
    assert_eq!(s.view.outline.offset, 2.0);
    assert!(s.view.hidden);
    assert_eq!(s.view.cursor, Cursor::Pointer);
}

#[test]
fn transitions_and_animations() {
    let s = styled("transition-colors duration-300 ease-out delay-75");
    let t = s.view.transition.unwrap();
    assert_eq!(t.duration, 300.0);
    assert_eq!(t.delay, 75.0);
    assert_eq!(t.easing, [0.0, 0.0, 0.2, 1.0]);
    assert!(t.properties.has(crate::effects::TransitionProps::COLORS));
    assert!(!t.properties.has(crate::effects::TransitionProps::TRANSFORM));
    let s = styled("transition");
    assert_eq!(s.view.transition.unwrap().duration, 150.0);
    let s = styled("animate-spin");
    assert_eq!(s.view.animation.unwrap().keyframes, Keyframes::Spin);
}

#[test]
fn display() {
    // As in CSS, a flex container is a row unless a direction utility says
    // otherwise, wherever that utility is in the string.
    let s = styled("flex");
    assert_eq!(s.layout.display, taffy::Display::Flex);
    assert_eq!(s.layout.flex_direction, taffy::FlexDirection::Row);
    assert_eq!(
        styled("inline-flex").layout.flex_direction,
        taffy::FlexDirection::Row
    );
    assert_eq!(
        styled("flex-col flex").layout.flex_direction,
        taffy::FlexDirection::Column
    );
    assert_eq!(
        styled("flex flex-col").layout.flex_direction,
        taffy::FlexDirection::Column
    );
    assert_eq!(
        styled("flex-col-reverse flex").layout.flex_direction,
        taffy::FlexDirection::ColumnReverse
    );
    // Without `flex`, views keep React Native's column.
    assert_eq!(
        styled("items-center").layout.flex_direction,
        taffy::FlexDirection::Column
    );
    // A variant `flex` is a row where it applies; a plain direction still wins.
    let (env, states) = cx(0);
    let wide = |c| styled_with(c, env, (800.0, 844.0), states).0;
    assert_eq!(
        wide("hidden md:flex").layout.flex_direction,
        taffy::FlexDirection::Row
    );
    assert_eq!(
        wide("flex-col md:flex").layout.flex_direction,
        taffy::FlexDirection::Column
    );
    assert_eq!(
        styled("hidden md:flex").layout.display,
        taffy::Display::None
    );
    assert_eq!(
        styled("hidden md:flex").layout.flex_direction,
        taffy::FlexDirection::Column
    );
    // Block formatting contexts use Taffy's block layout.
    assert_eq!(styled("block").layout.display, taffy::Display::Block);
    assert_eq!(styled("flow-root").layout.display, taffy::Display::Block);
    assert_eq!(styled("inline-block").layout.display, taffy::Display::Flex);
}

#[test]
fn grid() {
    let s = styled("grid grid-cols-3 gap-2 col-span-2");
    assert_eq!(s.layout.display, taffy::Display::Grid);
    assert_eq!(s.layout.grid_template_columns.len(), 1);
    assert_eq!(s.layout.grid_column.start, taffy::GridPlacement::Span(2));
}

#[test]
fn variants() {
    let theme = Theme::default();
    let list = parse(
        &theme,
        "bg-white dark:bg-black md:p-8 hover:opacity-80 group-active:scale-95 ios:pt-1 android:pt-2 rtl:ml-2",
    );
    assert!(list.unknown().is_empty(), "{:?}", list.unknown());
    let d = list.deps();
    assert!(
        d & deps::DARK != 0 && d & deps::WIDTH != 0 && d & deps::STATE != 0 && d & deps::GROUP != 0
    );
    assert_eq!(list.tracked_states(), state::HOVERED);
    let dark = ClassEnvironment {
        dark: true,
        platform: Platform::Android,
        reduce_motion: false,
    };
    let (s, _) = styled_with("bg-white dark:bg-black", dark, (390.0, 844.0), 0);
    assert_eq!(s.view.background_color, Color(0x0000_00ff));
    let (s, _) = styled_with("p-2 md:p-8", ClassEnvironment::default(), (390.0, 844.0), 0);
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(8.0));
    let (s, _) = styled_with(
        "p-2 md:p-8",
        ClassEnvironment::default(),
        (1024.0, 768.0),
        0,
    );
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(32.0));
    let (s, _) = styled_with(
        "p-2 max-md:p-1",
        ClassEnvironment::default(),
        (390.0, 844.0),
        0,
    );
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(4.0));
    let (s, _) = styled_with("pt-1 ios:pt-3 android:pt-5", dark, (390.0, 844.0), 0);
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(20.0));
    let web = ClassEnvironment {
        platform: Platform::Web,
        ..ClassEnvironment::default()
    };
    let (s, _) = styled_with("pt-1 ios:pt-3 web:pt-5", web, (390.0, 844.0), 0);
    assert_eq!(s.layout.padding.top, taffy::LengthPercentage::length(20.0));
    let (s, _) = styled_with(
        "w-screen h-screen",
        ClassEnvironment::default(),
        (390.0, 844.0),
        0,
    );
    assert_eq!(s.layout.size.width, taffy::Dimension::length(390.0));
    assert_eq!(s.layout.size.height, taffy::Dimension::length(844.0));
}

#[test]
fn groups() {
    let theme = Theme::default();
    let root = parse(&theme, "group/card p-2");
    assert_eq!(root.groups(), ["card".into()] as [Box<str>; 1]);
    let child = parse(&theme, "opacity-100 group-hover/card:opacity-50");
    let mut style = Style::default();
    let mut props = ClassProps::default();
    let group = |name: &str| if name == "card" { state::HOVERED } else { 0 };
    let context = Context {
        env: ClassEnvironment::default(),
        window: (390.0, 844.0),
        rtl: false,
        states: 0,
        position: 0,
        group: &group,
    };
    child.apply(&context, &theme, &mut style, &mut props, |_| {});
    assert_eq!(style.view.opacity, 0.5);
}

#[test]
fn unknown_classes_are_reported() {
    let theme = Theme::default();
    let list = parse(&theme, "p-4 bg-nope-500 flexx hover:wat antialiased");
    let unknown: Vec<&str> = list.unknown().iter().map(|s| &**s).collect();
    assert_eq!(unknown, ["bg-nope-500", "flexx", "hover:wat"]);
}

#[test]
fn placeholder_and_props() {
    let (_, props) = styled_with(
        "placeholder:text-zinc-400 selection:text-blue-500 caret-blue-500 line-clamp-2 pointer-events-none object-contain select-none",
        ClassEnvironment::default(),
        (390.0, 844.0),
        0,
    );
    assert_eq!(props.placeholder_color, Some(Color(0x9f9f_a9ff)));
    assert_eq!(props.caret_color, Some(Color(0x2b7f_ffff)));
    assert_eq!(props.selection_color, Some(Color(0x2b7f_ffff)));
    assert_eq!(props.number_of_lines, Some(2));
    assert_eq!(
        props.pointer_events,
        Some(crate::props::PointerEvents::None)
    );
    assert_eq!(props.resize_mode, Some(crate::ops::ResizeMode::Contain));
    assert_eq!(props.selectable, Some(false));
}
