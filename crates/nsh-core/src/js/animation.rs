//! The easings of native animations and the layout animations, as JS
//! passes them (`Easing.quad`, `LayoutAnimation.Presets.spring`): their
//! names, parameters and defaults in one table each, from which they are
//! decoded, `nsh/animated` is made (`nsh-js` `animated.rs`) and
//! `animation.gen.ts` (the declarations) is generated.

use std::fmt::Write as _;

use super::host::{HostCx, HostInput, HostList, HostValue, field};
use crate::{
    LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition,
    animation::{Curve, EaseMode, Easing},
    ui::TsTypes,
};

/// A curve of `Easing`: its name, its parameter (name and default) and
/// documentation.
struct CurveSpec {
    name: &'static str,
    param: Option<(&'static str, f32)>,
    make: fn(f32) -> Curve,
    doc: &'static str,
}

const CURVES: &[CurveSpec] = &[
    CurveSpec {
        name: "quad",
        param: None,
        make: |_| Curve::Quad,
        doc: "`t²`.",
    },
    CurveSpec {
        name: "cubic",
        param: None,
        make: |_| Curve::Cubic,
        doc: "`t³`.",
    },
    CurveSpec {
        name: "poly",
        param: Some(("n", 2.0)),
        make: Curve::Poly,
        doc: "`tⁿ`.",
    },
    CurveSpec {
        name: "sin",
        param: None,
        make: |_| Curve::Sin,
        doc: "Sinusoidal.",
    },
    CurveSpec {
        name: "circle",
        param: None,
        make: |_| Curve::Circle,
        doc: "Circular.",
    },
    CurveSpec {
        name: "exp",
        param: None,
        make: |_| Curve::Exp,
        doc: "Exponential.",
    },
    CurveSpec {
        name: "elastic",
        param: Some(("bounciness", 1.0)),
        make: Curve::Elastic,
        doc: "Springy overshoot.",
    },
    CurveSpec {
        name: "back",
        param: Some(("s", 1.70158)),
        make: Curve::Back,
        doc: "Pulls back before going; `s` is the overshoot.",
    },
    CurveSpec {
        name: "bounce",
        param: None,
        make: |_| Curve::Bounce,
        doc: "Bounces at the end.",
    },
];

/// The easings by name.
const NAMED: &[(&str, Easing)] = &[
    ("linear", Easing::Linear),
    // CSS / React Native `ease`.
    ("ease", Easing::Bezier(0.25, 0.1, 0.25, 1.0)),
    ("easeIn", Easing::EaseIn),
    ("easeOut", Easing::EaseOut),
    ("easeInOut", Easing::EaseInOut),
];

/// A name, a cubic Bézier `[x1, y1, x2, y2]` or `{ curve, param, mode }`;
/// `None` for anything else (the animation's default).
pub fn easing<I: HostInput>(input: &I) -> Option<Easing> {
    if let Some(list) = input.as_list() {
        let p = |i| list.item(i).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
        return Some(Easing::Bezier(p(0), p(1), p(2), p(3)));
    }
    if input.is_object() {
        let name = input.field("curve").and_then(|c| c.as_string())?;
        let spec = CURVES.iter().find(|c| c.name == name)?;
        let param = input.field("param").and_then(|p| p.as_number());
        let curve = (spec.make)(param.map_or_else(
            || spec.param.map_or(0.0, |(_, default)| default),
            |p| p as f32,
        ));
        let mode = match input.field("mode").and_then(|m| m.as_string()).as_deref() {
            Some("out") => EaseMode::Out,
            Some("inOut") => EaseMode::InOut,
            _ => EaseMode::In,
        };
        return Some(Easing::Curve(curve, mode));
    }
    let name = input.as_string()?;
    NAMED.iter().find(|(n, _)| *n == name).map(|&(_, e)| e)
}

/// Makes a layout curve (from `springDamping`).
type MakeLayoutCurve = fn(f32) -> LayoutCurve;

/// The curves of layout animation phases (`type`), by name; the first is
/// the default.
const LAYOUT_TYPES: &[(&str, MakeLayoutCurve)] = &[
    ("easeInEaseOut", |_| LayoutCurve::EaseInEaseOut),
    ("spring", |damping| LayoutCurve::Spring { damping }),
    ("linear", |_| LayoutCurve::Linear),
    ("easeIn", |_| LayoutCurve::EaseIn),
    ("easeOut", |_| LayoutCurve::EaseOut),
];

/// What appearing and disappearing views animate (`property`), by name
/// (and alias: `false`); the first is the default.
const LAYOUT_PROPERTIES: &[(&str, LayoutProperty, bool)] = &[
    ("opacity", LayoutProperty::Opacity, true),
    ("scaleXY", LayoutProperty::Scale, true),
    ("scale", LayoutProperty::Scale, false),
];

/// React Native's default damping of layout springs.
const SPRING_DAMPING: f32 = 0.7;

/// One phase of `{ duration, create, update, delete }`.
fn transition<I: HostInput>(part: &I, duration: f32, cx: &mut HostCx) -> LayoutTransition {
    // The entry named by `key`, or the first.
    let named = |key, names: &mut dyn Iterator<Item = &'static str>| {
        let name = part.field(key).and_then(|v| v.as_string());
        let mut at = 0;
        for (i, n) in names.enumerate() {
            if Some(n) == name.as_deref() {
                at = i;
                break;
            }
        }
        at
    };
    let damping = field::<f32, _>(part, "springDamping", cx)
        .unwrap_or(SPRING_DAMPING)
        .clamp(0.01, 1.0);
    let curve = LAYOUT_TYPES[named("type", &mut LAYOUT_TYPES.iter().map(|t| t.0))].1(damping);
    let property =
        LAYOUT_PROPERTIES[named("property", &mut LAYOUT_PROPERTIES.iter().map(|p| p.0))].1;
    LayoutTransition {
        curve,
        property,
        duration_ms: field(part, "duration", cx).unwrap_or(duration),
        delay_ms: field(part, "delay", cx).unwrap_or(0.0),
    }
}

/// `LayoutAnimation.configureNext(config)`: `{ duration, create, update,
/// delete }`, each phase `{ type, property, duration, delay, springDamping
/// }` (without one, that kind of change is not animated).
impl HostValue for LayoutAnimation {
    fn decode<I: HostInput>(input: &I, cx: &mut HostCx) -> Option<Self> {
        let duration = field(input, "duration", cx).unwrap_or(300.0);
        let mut phase = |key| {
            input
                .field(key)
                .filter(|p| p.is_object())
                .map(|p| transition(&p, duration, cx))
        };
        Some(LayoutAnimation {
            create: phase("create"),
            update: phase("update"),
            delete: phase("delete"),
        })
    }

    fn ts_type(types: &mut TsTypes) -> String {
        let union = |names: &mut dyn Iterator<Item = &str>| {
            names
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let curve = types.declare("LayoutAnimationType", |_| {
            format!(
                "/** The curve of a layout animation phase (`type`). */\nexport type LayoutAnimationType = {};",
                union(&mut LAYOUT_TYPES.iter().map(|t| t.0))
            )
        });
        let property = types.declare("LayoutAnimationProperty", |_| {
            format!(
                "/** What appearing and disappearing views animate (`property`). */\n\
                 export type LayoutAnimationProperty = {};",
                union(&mut LAYOUT_PROPERTIES.iter().map(|p| p.0))
            )
        });
        types.declare("LayoutAnimationAnimation", |_| {
            format!(
                "/** How one kind of change animates. */\nexport interface LayoutAnimationAnimation {{\n  \
                 type?: {curve};\n  /** What appearing and disappearing views animate. */\n  \
                 property?: {property};\n  /** Milliseconds (the config's by default). */\n  \
                 duration?: number;\n  delay?: number;\n  /** The `spring`'s damping ratio, 0..1 \
                 ({SPRING_DAMPING} by default; 1 does not overshoot). */\n  springDamping?: number;\n}}"
            )
        });
        types.declare("LayoutAnimationConfig", |_| {
            "/** What the next commit's changes animate: views inserted (`create`), moved or \
             resized (`update`) and removed (`delete`). */\nexport interface LayoutAnimationConfig {\n  \
             /** Milliseconds. */\n  duration: number;\n  create?: LayoutAnimationAnimation;\n  \
             update?: LayoutAnimationAnimation;\n  delete?: LayoutAnimationAnimation;\n}"
                .into()
        })
    }
}

/// The presets, by their JS names.
const PRESETS: &[(&str, LayoutAnimation)] = &[
    ("easeInEaseOut", LayoutAnimation::EASE_IN_EASE_OUT),
    ("linear", LayoutAnimation::LINEAR),
    ("spring", LayoutAnimation::SPRING),
];

/// The declarations of the easing and layout animation types (`nsh/animated`
/// declares the values: `Easing`, `LayoutAnimation`).
pub fn declarations() -> String {
    let mut types = TsTypes::default();
    let layout = <LayoutAnimation as HostValue>::ts_type(&mut types);
    let names = |list: &mut dyn Iterator<Item = &str>| {
        list.map(|n| format!("{n:?}"))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "/** The curves of `Easing` (their \"in\" form; `Easing.out` / `inOut` turn them around). */\n\
         export type EasingCurveName = {};\n\n\
         /** A curve of the `Easing` module. */\n\
         export interface EasingCurve {{\n  curve: EasingCurveName;\n  param?: number;\n  mode: \"in\" | \"out\" | \"inOut\";\n}}\n\n\
         /** How a timing animation (or an interpolation segment) progresses: a name, a cubic\n \
         * Bézier `[x1, y1, x2, y2]` or a curve from `Easing`. */\n\
         export type Easing = {} | [number, number, number, number] | EasingCurve;\n",
        names(&mut CURVES.iter().map(|c| c.name)),
        names(&mut NAMED.iter().map(|(n, _)| *n)),
    );
    out.push_str("\n/** `Easing`'s curves. */\nexport interface EasingCurves {\n");
    for c in CURVES {
        match c.param {
            Some((p, default)) => {
                let _ = writeln!(
                    out,
                    "  /** {} `{p}`: {default} by default. */\n  {}({p}?: number): EasingCurve;",
                    c.doc, c.name
                );
            }
            None => {
                let _ = writeln!(out, "  /** {} */\n  {}: EasingCurve;", c.doc, c.name);
            }
        }
    }
    out.push_str("}\n");
    for declaration in types.declarations() {
        let _ = write!(out, "\n{declaration}\n");
    }
    let _ = write!(
        out,
        "\n/** Ready-made layout animations. */\nexport interface LayoutAnimationPresets {{\n{}}}\n",
        PRESETS
            .iter()
            .map(|(name, _)| format!("  {name}: {layout};\n"))
            .collect::<String>()
    );
    // `LayoutAnimation.Types` / `Properties`: each name as itself.
    let object = |iface: &str, doc: &str, names: &mut dyn Iterator<Item = &str>| {
        let fields: String = names.map(|n| format!("  readonly {n}: {n:?};\n")).collect();
        format!("\n/** {doc} */\nexport interface {iface} {{\n{fields}}}\n")
    };
    out.push_str(&object(
        "LayoutAnimationTypes",
        "The curves, by name.",
        &mut LAYOUT_TYPES.iter().map(|t| t.0),
    ));
    out.push_str(&object(
        "LayoutAnimationProperties",
        "The properties, by name.",
        &mut LAYOUT_PROPERTIES.iter().filter(|p| p.2).map(|p| p.0),
    ));
    out
}

/// The easing and layout animation tables as data for `nsh/animated`
/// (`nsh-js` `animated.rs`): `curves` (`[name, parameter,
/// default]`, the parameter `null` for curves without one), `presets`
/// (name -> config, as `LayoutAnimation.Presets`), `types` and
/// `properties` (the names of `LayoutAnimation.Types` / `Properties`).
pub fn tables() -> crate::JsonValue {
    use serde_json::{Map, Value, json};
    let transition = |t: &LayoutTransition| {
        let (kind, damping) = match t.curve {
            LayoutCurve::Linear => ("linear", None),
            LayoutCurve::EaseIn => ("easeIn", None),
            LayoutCurve::EaseOut => ("easeOut", None),
            LayoutCurve::EaseInEaseOut => ("easeInEaseOut", None),
            LayoutCurve::Spring { damping } => ("spring", Some(damping)),
        };
        let property = match t.property {
            LayoutProperty::Opacity => "opacity",
            LayoutProperty::Scale => "scaleXY",
        };
        let mut o = json!({
            "type": kind,
            "property": property,
            "duration": t.duration_ms,
            "delay": t.delay_ms,
        });
        if let Some(d) = damping {
            o["springDamping"] = json!(d);
        }
        o
    };
    let mut presets = Map::new();
    for (name, preset) in PRESETS {
        let duration = [preset.create, preset.update, preset.delete]
            .iter()
            .flatten()
            .map(|t| t.duration_ms + t.delay_ms)
            .fold(0.0, f32::max);
        let mut o = Map::new();
        o.insert("duration".into(), json!(duration));
        for (key, phase) in [
            ("create", &preset.create),
            ("update", &preset.update),
            ("delete", &preset.delete),
        ] {
            if let Some(t) = phase {
                o.insert(key.into(), transition(t));
            }
        }
        presets.insert((*name).into(), Value::Object(o));
    }
    json!({
        "curves": CURVES
            .iter()
            .map(|c| match c.param {
                Some((p, default)) => json!([c.name, p, default]),
                None => json!([c.name, null, null]),
            })
            .collect::<Vec<_>>(),
        "presets": presets,
        "types": LAYOUT_TYPES.iter().map(|t| t.0).collect::<Vec<_>>(),
        "properties": LAYOUT_PROPERTIES.iter().filter(|p| p.2).map(|p| p.0).collect::<Vec<_>>(),
    })
}
