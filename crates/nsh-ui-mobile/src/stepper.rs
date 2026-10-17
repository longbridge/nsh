//! `Stepper`: a number with − and + buttons; holding one keeps stepping.

use std::{rc::Rc, time::Duration};

use nsh_core::ui::prelude::*;

/// A number with − and + buttons, within `min`–`max`; holding a button
/// keeps stepping. With `value` the app controls it (it updates `value`
/// from `onChange`); without, the stepper keeps its own, starting at
/// `defaultValue`.
#[derive(Debug, Clone, Copy, PartialEq, Props)]
pub struct StepperProps {
    /// The value, when the app controls it.
    pub value: Prop<Option<f64>>,
    /// The first value, when the app does not control it.
    #[prop(default)]
    pub default_value: Prop<f64>,
    /// The smallest value.
    #[prop(default = 0.0)]
    pub min: Prop<f64>,
    /// The largest value.
    #[prop(default = 100.0)]
    pub max: Prop<f64>,
    /// How much a press changes it.
    #[prop(default = 1.0)]
    pub step: Prop<f64>,
    /// No presses, dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
}

/// The events of a [`Stepper`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum StepperEvent {
    /// The value changed (the new one).
    Change(f64),
}

/// How long a press is held before it repeats, and how fast it repeats.
const HOLD: Duration = Duration::from_millis(400);
const REPEAT: Duration = Duration::from_millis(80);

/// The bounds and the step.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    min: f64,
    max: f64,
    step: f64,
    controlled: bool,
}

impl Range {
    fn of(props: &StepperProps) -> Self {
        let (min, max) = (props.min.get(), props.max.get());
        Self {
            min: min.min(max),
            max: min.max(max),
            step: props.step.get().abs(),
            controlled: props.value.with(Option::is_some),
        }
    }

    /// `value` on the step grid within the bounds.
    fn fit(&self, value: f64) -> f64 {
        let decimals = decimals(self.step);
        let scale = 10f64.powi(decimals as i32);
        ((value * scale).round() / scale).clamp(self.min, self.max)
    }
}

/// The decimals of `step` (0.25 has 2), to show and round to.
fn decimals(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.').map_or(0, |(_, d)| d.len())
}

/// What JS calls on a stepper's ref.
pub struct StepperApi {
    value: Memo<f64>,
    range: Memo<Range>,
    change: Rc<dyn Fn(f64)>,
}

#[methods]
impl StepperApi {
    /// Sets the value (on the step grid, within the bounds) and reports it
    /// as a press would.
    pub fn set_value(&self, value: f64) {
        (self.change)(self.range.get_untracked().fit(value));
    }

    /// The value shown.
    pub fn value(&self) -> f64 {
        self.value.get_untracked()
    }
}

/// A number with − and + buttons ([`StepperProps`]).
#[component(methods = StepperApi)]
pub fn Stepper(props: StepperProps, cx: Scope<StepperEvent>) -> impl IntoElement {
    let range = computed(move || Range::of(&props));
    // Our value, when the app does not control it; on the grid and within
    // the bounds either way.
    let own = signal(untracked(|| range.get().fit(props.default_value.get())));
    let value = computed(move || range.get().fit(props.value.get().unwrap_or(own.get())));
    // Stepping while a button is held.
    let repeat = stored(None::<Spawned<()>>);
    let stop = move || {
        if let Some(task) = repeat.replace(None) {
            task.cancel();
        }
    };
    // Shows `next` (unless the app controls the value) and reports it.
    let change = move |next: f64| {
        if next == value.get_untracked() {
            stop();
            return;
        }
        // The app's value follows from `onChange` when it controls it.
        if !range.get_untracked().controlled {
            own.set(next);
        }
        cx.emit(StepperEvent::Change(next));
    };
    // One step of `direction` (−1 or 1); stops repeating at a bound.
    let step = move |direction: f64| {
        let range = range.get_untracked();
        change(range.fit(value.get_untracked() + direction * range.step));
        let now = value.get_untracked();
        if now == range.min || now == range.max {
            stop();
        }
    };
    // A press on − or +: one step now, more while it is held.
    let press = move |direction: f64| {
        stop();
        step(direction);
        repeat.replace(Some(spawn(async move {
            timer(HOLD).await;
            loop {
                step(direction);
                timer(REPEAT).await;
            }
        })));
    };
    effect(move || {
        if props.disabled.get() {
            stop();
        }
    });
    cx.methods(StepperApi {
        value,
        range,
        change: Rc::new(change),
    });
    let button =
        move |label: &'static str, name: &'static str, direction: f64, at_bound: Memo<bool>| {
            let off = computed(move || props.disabled.get() || at_bound.get());
            view()
                .size_10()
                .items_center()
                .justify_center()
                .rounded(move || px(theme().radius))
                .bg(move || theme().secondary)
                .active(|s| s.bg(move || theme().secondary_active))
                .style_when(off, |v| v.opacity(0.4))
                .disabled(off)
                .role(AccessibilityRole::Button)
                .label(name)
                .on(EventKind::PressIn, move |_: &EventPayload| press(direction))
                .on(EventKind::PressOut, move |_: &EventPayload| stop())
                .child(
                    text(label)
                        .text_size(move || px(theme().font_size + 4.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().secondary_foreground),
                )
        };
    view()
        .flex_row()
        .items_center()
        .gap_3()
        .child(button(
            "−",
            "Decrease",
            -1.0,
            computed(move || value.get() <= range.get().min),
        ))
        .child(
            text(move || format!("{:.*}", decimals(range.get().step), value.get()))
                .min_w(px(48.0))
                .text_center()
                .text_size(move || px(theme().font_size + 1.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        )
        .child(button(
            "+",
            "Increase",
            1.0,
            computed(move || value.get() >= range.get().max),
        ))
}
