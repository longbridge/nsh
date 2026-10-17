//! Mobile UI: `nsh/ui/mobile`'s Rust components laying out the demo's
//! content (as the TSX gallery's `demos/MobileUi.tsx`): a `ListView`
//! recycles rows the demo renders, under a header of cards with a text
//! field, a sheet, a select list, groups, steppers and their methods; a
//! `Group` wraps each child in a row. The demo writes the content; the
//! components only place it. Their colors come from the app's theme, so
//! the text the demo puts inside them keeps the color they give it.

use std::rc::Rc;

use nsh_core::ui::prelude::*;
use nsh_ui_mobile::{
    Button, ButtonEvent, ButtonProps, ButtonVariant, Group, GroupProps, ListView, ListViewProps,
    ListViewVariant, SelectList, SelectListEvent, SelectListProps, Sheet, SheetEvent, SheetProps,
    Stepper, StepperApi, StepperEvent, StepperProps, TextField, TextFieldEvent, TextFieldProps,
};

use crate::ui::{
    Badge, Card, IconName, MONO, Row, ToggleEvent, Tone, appear, caption, colors, toggle,
};

/// A row of the list.
#[derive(Debug, Clone, PartialEq)]
struct Stock {
    id: usize,
    symbol: String,
    price: String,
    change: f64,
}

fn stocks() -> Vec<Stock> {
    (0..300)
        .map(|i| Stock {
            id: i,
            symbol: format!("SYM{i:03}"),
            price: format!(
                "{:.2}",
                50.0 + ((i * 37) % 400) as f64 + (i % 7) as f64 * 0.13
            ),
            change: (((i * 13) % 21) as f64 - 10.0) / 4.0,
        })
        .collect()
}

/// A mobile UI button titled `title` (a value, or bound).
fn mobile_button(title: Prop<SharedString>, variant: ButtonVariant) -> ComponentElement<Button> {
    Button::element(ButtonProps {
        title,
        variant: variant.into(),
        ..ButtonProps::default()
    })
}

/// A row of two things at its ends.
fn between() -> View {
    view().flex_row().items_center().justify_between()
}

/// The header's props.
#[derive(Debug, Clone, Copy, PartialEq, Props)]
struct HeaderProps {
    /// The page's rows are inset (the page keeps it: TS keeps it at
    /// module level).
    #[prop(default)]
    inset: Prop<bool>,
}

/// What the header asks of the page.
#[derive(Debug, Clone, PartialEq, Event)]
enum HeaderEvent {
    /// Show plain or inset rows.
    Inset(bool),
}

/// The list's header: the components, on cards.
#[component]
fn Header(props: HeaderProps, cx: Scope<HeaderEvent>) -> impl IntoElement {
    let cities: Rc<[SharedString]> = (1..=500).map(|i| format!("City {i}").into()).collect();
    let taps = signal(0_u32);
    // The controlled stepper's value.
    let quantity = signal(2.0_f64);
    // What asking the other stepper answered.
    let asked = signal(String::new());
    let name = signal("Ada".to_owned());
    let sheet = signal(false);
    let city = signal(None::<u32>);
    // The stepper with a value of its own, asked for it (TS: its ref).
    let own = component_ref::<Stepper>();
    let chosen_badge = {
        let cities = cities.clone();
        Badge::new(move || {
            city.get()
                .map_or_else(|| "none".into(), |i| cities[i as usize].clone())
        })
        .tone(move || {
            if city.with(Option::is_some) {
                Tone::Accent
            } else {
                Tone::Neutral
            }
        })
    };
    let ask = {
        let own = own.clone();
        move |_: &ButtonEvent| {
            // Methods of a Rust component, called from Rust.
            if let Some(stepper) = own.api::<StepperApi>() {
                let value = stepper.value();
                stepper.set_value(5.0);
                asked.set(format!("It was {value}; reset to 5"));
            }
        }
    };
    view()
        .p_4()
        .gap_4()
        .bg(colors::BACKGROUND)
        .child(
            appear(0).child(
                view()
                    .gap(px(6.0))
                    .px_1()
                    .child(
                        view()
                            .flex_row()
                            .gap_2()
                            .child(Badge::new("Rust").tone(Tone::Accent))
                            .child(Badge::new("nsh-ui-mobile")),
                    )
                    .child(
                        text(
                            "Components written in Rust, running in the core: the app hands them \
                             JSX and data, they draw the boxes and keep their own state.",
                        )
                        .text_size(px(15.0))
                        .line_height(px(21.0))
                        .text_color(colors::MUTED),
                    ),
            ),
        )
        .child(
            appear(1).child(
                Card::new()
                    .icon(IconName::Form)
                    .title("TextField")
                    .subtitle("A label, a clear button, controlled or not")
                    .child(
                        TextField::element(TextFieldProps {
                            label: "Name".into(),
                            placeholder: "Your name".into(),
                            value: Prop::bind(move || Some(SharedString::from(name.get()))),
                            ..TextFieldProps::default()
                        })
                        .on_event(move |e: &TextFieldEvent| {
                            if let TextFieldEvent::Change(v) = e {
                                name.set(v.to_string());
                            }
                        }),
                    )
                    .child(TextField::element(TextFieldProps {
                        label: "Password".into(),
                        placeholder: "At least 8 characters".into(),
                        secure: true.into(),
                        ..TextFieldProps::default()
                    }))
                    .child(caption(
                        move || {
                            name.with(|n| {
                                format!("Hello, {}", if n.is_empty() { "nobody" } else { n })
                            })
                        },
                        Some("× clears and focuses"),
                    )),
            ),
        )
        .child(
            appear(2).child(
                Card::new()
                    .icon(IconName::Components)
                    .title("Sheet")
                    .subtitle("Follows the finger natively, springs back")
                    .child(
                        mobile_button("Open the sheet".into(), ButtonVariant::Primary)
                            .on_event(move |_: &ButtonEvent| sheet.set(true)),
                    )
                    .child(caption("Drag, tap outside or press back", Some("onClose"))),
            ),
        )
        .child(
            Sheet::element(SheetProps {
                open: sheet.into(),
                ..SheetProps::default()
            })
            .on_event(move |_: &SheetEvent| sheet.set(false))
            .child(
                text("A sheet in Rust")
                    .text_size(px(17.0))
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(
                text(
                    "It follows the finger natively and springs back; drag it down, tap outside \
                     or press back to close.",
                )
                .opacity(0.7),
            )
            .child(
                mobile_button("Done".into(), ButtonVariant::Primary)
                    .on_event(move |_: &ButtonEvent| sheet.set(false)),
            ),
        )
        .child(
            appear(3).child(
                Card::new()
                    .icon(IconName::Lists)
                    .title("SelectList")
                    .subtitle("500 rows of Rust data")
                    .right(chosen_badge)
                    .flush()
                    .child(
                        view()
                            .h(px(240.0))
                            .rounded(px(15.0))
                            .border_curve(BorderCurve::Continuous)
                            .overflow_hidden()
                            .child(
                                SelectList::element(SelectListProps {
                                    options: cities.to_vec().into(),
                                    value: city.into(),
                                })
                                .on_event(
                                    move |e: &SelectListEvent| {
                                        let SelectListEvent::Select(i) = *e;
                                        city.set(Some(i));
                                    },
                                ),
                            ),
                    ),
            ),
        )
        .child(
            appear(4).child(
                Card::new()
                    .icon(IconName::Mobile)
                    .title("Group")
                    .subtitle("Each JSX child becomes a row")
                    .flush()
                    .child(
                        Group::element(GroupProps::default())
                            .fill("header", text("Settings"))
                            .fill(
                                "footer",
                                text("The rows are JSX; the group adds the box and the dividers."),
                            )
                            .child(text("Notifications"))
                            .child(text("Privacy"))
                            .child(
                                view()
                                    .flex_row()
                                    .justify_between()
                                    .child(text("Taps"))
                                    .child(text(move || taps.get().to_string()).tabular_nums()),
                            ),
                    ),
            ),
        )
        .child(
            appear(5).child(
                Card::new()
                    .icon(IconName::Plus)
                    .title("Stepper")
                    .subtitle("State and timers in Rust: hold a button")
                    .flush()
                    .child(
                        Group::element(GroupProps::default())
                            .child(
                                between().child(text("Own value")).child(
                                    Stepper::element(StepperProps {
                                        default_value: 5.0.into(),
                                        min: 0.0.into(),
                                        max: 50.0.into(),
                                        ..StepperProps::default()
                                    })
                                    .component_ref(&own),
                                ),
                            )
                            .child(
                                between()
                                    .child(
                                        text(move || format!("Controlled: {}", quantity.get()))
                                            .tabular_nums(),
                                    )
                                    .child(
                                        Stepper::element(StepperProps {
                                            value: Prop::bind(move || Some(quantity.get())),
                                            min: 0.0.into(),
                                            max: 10.0.into(),
                                            step: 0.5.into(),
                                            ..StepperProps::default()
                                        })
                                        .on_event(
                                            move |e: &StepperEvent| {
                                                let StepperEvent::Change(v) = *e;
                                                quantity.set(v);
                                            },
                                        ),
                                    ),
                            ),
                    ),
            ),
        )
        .child(
            appear(6).child(
                Card::new()
                    .icon(IconName::Spark)
                    .title("Methods")
                    .subtitle("A Rust component's ref, called from Rust")
                    .child(
                        mobile_button(
                            Prop::bind(move || {
                                asked.with(|a| {
                                    if a.is_empty() {
                                        "Ask the stepper, then reset it".into()
                                    } else {
                                        a.clone().into()
                                    }
                                })
                            }),
                            ButtonVariant::Secondary,
                        )
                        .on_event(ask),
                    )
                    .child(
                        mobile_button(
                            Prop::bind(move || format!("Tap ({})", taps.get()).into()),
                            ButtonVariant::Secondary,
                        )
                        .on_event(move |_: &ButtonEvent| taps.update(|t| *t += 1)),
                    )
                    .child(
                        view().child(
                            Row::new(true)
                                .child(
                                    view()
                                        .flex_1()
                                        .gap(px(2.0))
                                        .child(
                                            text("Inset rows")
                                                .text_size(px(15.0))
                                                .text_color(colors::TEXT),
                                        )
                                        .child(
                                            text(move || {
                                                format!(
                                                    "variant=\"{}\"",
                                                    if props.inset.get() {
                                                        "inset"
                                                    } else {
                                                        "plain"
                                                    }
                                                )
                                            })
                                            .text_size(px(12.0))
                                            .text_color(colors::FAINT)
                                            .font_family(MONO),
                                        ),
                                )
                                .child(toggle(props.inset, "Inset rows").on_event(
                                    move |e: &ToggleEvent| {
                                        let ToggleEvent::Change(on) = *e;
                                        cx.emit(HeaderEvent::Inset(on));
                                    },
                                )),
                        ),
                    ),
            ),
        )
        .child(
            text("Below: a ListView of 300 rows the app renders, recycled natively")
                .text_size(px(13.0))
                .text_color(colors::MUTED)
                .px_1(),
        )
}

/// The demo's page: the stocks in a `ListView`, under the header.
#[component]
pub(crate) fn MobileUi() -> impl IntoElement {
    let stocks = signal(stocks());
    let inset = signal(false);
    // White on the fills in light appearance, the inset's black in dark.
    let on_fill = AdaptiveColor::new(rgb(0xffffff), colors::INSET.dark);
    // A cell's content is made once; it shows the stock at its position.
    let rows = virtual_list(stocks.with_untracked(Vec::len), move |at| {
        let field = move |f: fn(&Stock) -> String| move || stocks.with(|s| f(&s[at.index()]));
        let up = computed(move || stocks.with(|s| s[at.index()].change >= 0.0));
        view()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                view()
                    .flex_1()
                    .gap(px(1.0))
                    .child(
                        text(field(|s| s.symbol.clone()))
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        text(field(|s| format!("No. {}", s.id + 1)))
                            .text_size(px(12.0))
                            .opacity(0.55)
                            .tabular_nums(),
                    ),
            )
            .child(
                text(field(|s| s.price.clone()))
                    .w(px(80.0))
                    .text_right()
                    .text_size(px(15.0))
                    .tabular_nums(),
            )
            .child(
                view()
                    .w(px(72.0))
                    .h(px(28.0))
                    .rounded(px(8.0))
                    .items_center()
                    .justify_center()
                    .bg(move || if up.get() { colors::UP } else { colors::DOWN })
                    .child(
                        text(field(|s| {
                            format!("{}{:.2}%", if s.change >= 0.0 { "+" } else { "" }, s.change)
                        }))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(on_fill)
                        .tabular_nums(),
                    ),
            )
    })
    .keys(move || stocks.with(|s| s.iter().map(|s| s.id).collect::<Vec<_>>()))
    .header(
        Header::element(HeaderProps {
            inset: inset.into(),
        })
        .on_event(move |e: &HeaderEvent| {
            let HeaderEvent::Inset(on) = *e;
            inset.set(on);
        }),
    );
    view().flex_1().child(
        ListView::element(ListViewProps {
            variant: Prop::bind(move || {
                if inset.get() {
                    ListViewVariant::Inset
                } else {
                    ListViewVariant::Plain
                }
            }),
            ..ListViewProps::default()
        })
        .items(rows),
    )
}
