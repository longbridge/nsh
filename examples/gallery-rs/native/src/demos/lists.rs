//! Lists: three columns with a header of the viewable rows, a footer and
//! jumps, contacts under sticky letters, an inverted chat that loads older
//! messages, horizontal lists, pull to refresh on a scroll view and an
//! empty list with separators, each under a tab (as the TSX gallery's
//! `demos/Lists.tsx`).

use std::{rc::Rc, time::Duration};

use nsh_core::ui::{LayoutAnimation, prelude::*};
use nsh_ui_mobile::{KeyboardAvoidingView, KeyboardAvoidingViewProps};

use crate::ui::{
    Appear, AppearProps, Badge, Button, ButtonVariant, Finish, Finished, IconName, IconTile, MONO,
    Meter, PressProps, PressRole, RoundButton, Section, Segment, SegmentedEvent, Tone, colors,
    icon, press_with, radius, segmented,
};

/// The tones rows cycle through (the last, neutral, only in the grid).
const TONES: [Tone; 6] = [
    Tone::Accent,
    Tone::Info,
    Tone::Warn,
    Tone::Up,
    Tone::Danger,
    Tone::Neutral,
];

/// A tone's name (TS: the `Tone` string).
fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "neutral",
        Tone::Accent => "accent",
        Tone::Info => "info",
        Tone::Warn => "warn",
        Tone::Danger => "danger",
        Tone::Up => "up",
        Tone::Down => "down",
    }
}

/// A small pill action for list headers (36 pt, 44 pt with its hit slop).
fn action(
    label: &'static str,
    icon_name: Option<IconName>,
    on_press: impl Into<Listener<()>>,
) -> impl IntoElement {
    press_with(
        PressProps {
            label: Some(label.into()).into(),
            hit_slop: Some(4.0),
            ..PressProps::default()
        },
        on_press,
    )
    .child(
        view()
            .h(px(36.0))
            .px(px(14.0))
            .rounded(px(18.0))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .bg(colors::RAISED)
            .border_1()
            .border_color(colors::BORDER)
            .finish(Finish::Raised)
            .children(icon_name.map(|name| icon(name, 14.0, colors::TEXT)))
            .child(
                text(label)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::TEXT),
            ),
    )
}

/// The grid's items: 90, of four heights.
const ITEMS: usize = 90;

/// The grid's item `id`'s height, in points.
fn item_height(id: usize) -> usize {
    84 + (id % 4) * 22
}

/// The grid's header: the viewable rows (`visible`, the first and the
/// last), a meter of how far down they are, and the jumps.
fn grid_header(visible: Signal<Option<(usize, usize)>>, list: &ElementRef) -> impl IntoElement {
    let progress = computed(move || {
        visible
            .get()
            .map_or(0.0, |(_, last)| (last + 1) as f32 / ITEMS as f32)
    });
    let (jump, end) = (list.clone(), list.clone());
    view()
        .bg(colors::SURFACE)
        .rounded(px(radius::CARD))
        .border_curve(BorderCurve::Continuous)
        .border_1()
        .border_color(colors::BORDER)
        .p_4()
        .gap_3()
        .mb_1()
        .finish(Finish::Card)
        .child(
            view()
                .flex_row()
                .items_end()
                .gap_2()
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(
                            text("Viewable items (at least half shown)")
                                .text_size(px(13.0))
                                .text_color(colors::MUTED),
                        )
                        .child(
                            text(move || match visible.get() {
                                Some((first, last)) => format!("{first}–{last}"),
                                None => "—".to_owned(),
                            })
                            .text_size(px(30.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors::TEXT)
                            .tabular_nums()
                            .letter_spacing(-0.5)
                            .child(
                                text(move || {
                                    visible.get().map_or(String::new(), |(first, last)| {
                                        format!("  {} visible", last - first + 1)
                                    })
                                })
                                .text_size(px(15.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors::ACCENT),
                            ),
                        ),
                )
                .child(
                    text("90 items, 3 columns")
                        .text_size(px(13.0))
                        .text_color(colors::FAINT)
                        .pb(px(6.0)),
                ),
        )
        .child(Meter::new(progress))
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(action(
                    "Jump to 60",
                    Some(IconName::ArrowDown),
                    move |_: &()| jump.scroll_to_index(60, 0.5, 0.0, true),
                ))
                .child(action("End", None, move |_: &()| end.scroll_to_end(true))),
        )
}

/// Three columns, a header and a footer, padding and gaps, viewability.
#[component]
fn Grid() -> impl IntoElement {
    // The first and last viewable rows.
    let visible = signal(None::<(usize, usize)>);
    let list = element_ref();
    let header = grid_header(visible, &list);
    view().flex_1().child(
        // A cell shows other items as it is reused: what differs between
        // items is bound to its position.
        virtual_list(ITEMS, |at| {
            view()
                .h(move || px(item_height(at.index()) as f32))
                .rounded(px(radius::INSET))
                .border_curve(BorderCurve::Continuous)
                .border_1()
                .border_color(move || {
                    if at.index() == 60 {
                        Tone::Accent.color()
                    } else {
                        colors::BORDER
                    }
                })
                .bg(colors::RAISED)
                .finish(Finish::Raised)
                .p_3()
                .justify_between()
                .child(
                    view()
                        .w(px(18.0))
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(move || TONES[at.index() % TONES.len()].color()),
                )
                .child(
                    view()
                        .child(
                            text(move || at.index().to_string())
                                .text_size(px(22.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors::TEXT)
                                .tabular_nums(),
                        )
                        .child(
                            text(move || format!("{} pt", item_height(at.index())))
                                .text_size(px(12.0))
                                .text_color(colors::FAINT)
                                .tabular_nums(),
                        ),
                )
        })
        .columns(3)
        .estimated_item_size(110.0)
        .content_padding(16.0)
        .row_gap(8.0)
        .column_gap(8.0)
        .viewability(Viewability {
            item_visible_percent: Some(50.0),
            ..Viewability::default()
        })
        .on_viewable_items_changed(move |rows: &ViewableItems| visible.set(rows.viewable))
        .header(header)
        .footer(
            text("End of the list")
                .text_center()
                .text_color(colors::FAINT)
                .text_size(px(13.0))
                .p_4(),
        )
        .flex_1()
        .element_ref(&list),
    )
}

/// The people of the contacts: first and last name, what they made.
const PEOPLE: [(&str, &str, &str); 15] = [
    ("Ada", "Lovelace", "Analytical Engine notes"),
    ("Alan", "Kay", "Smalltalk"),
    ("Barbara", "Liskov", "CLU, substitution"),
    ("Brian", "Kernighan", "AWK, The C book"),
    ("Claude", "Shannon", "Information theory"),
    ("Dennis", "Ritchie", "C, Unix"),
    ("Donald", "Knuth", "TeX, TAOCP"),
    ("Edsger", "Dijkstra", "Shortest paths"),
    ("Frances", "Allen", "Optimizing compilers"),
    ("Grace", "Hopper", "COBOL, the first compiler"),
    ("Guido", "van Rossum", "Python"),
    ("Ken", "Thompson", "Unix, UTF-8"),
    ("Linus", "Torvalds", "Linux, Git"),
    ("Margaret", "Hamilton", "Apollo flight software"),
    ("Niklaus", "Wirth", "Pascal, Oberon"),
];

/// A person of the contacts, as their row shows them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Person {
    first: &'static str,
    last: &'static str,
    note: &'static str,
    tone: Tone,
    /// The last of its letter: no hairline under it.
    end: bool,
}

/// A row of the contacts: a letter's header or a person.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ContactRow {
    Header { letter: char, count: usize },
    Person(Person),
}

impl ContactRow {
    /// The letter and its count (nothing for a person's row).
    fn header(self) -> (char, usize) {
        match self {
            ContactRow::Header { letter, count } => (letter, count),
            ContactRow::Person(_) => (' ', 0),
        }
    }

    /// The person (nobody for a header's row).
    fn person(self) -> Person {
        match self {
            ContactRow::Person(person) => person,
            ContactRow::Header { .. } => Person::default(),
        }
    }
}

/// The contacts' rows: each letter's header, then its people.
fn contact_rows() -> Vec<ContactRow> {
    let initial = |name: &str| name.chars().next().unwrap_or(' ');
    let mut letters: Vec<char> = PEOPLE.iter().map(|p| initial(p.0)).collect();
    letters.dedup();
    let mut rows = Vec::new();
    for letter in letters {
        let group: Vec<_> = PEOPLE.iter().filter(|p| initial(p.0) == letter).collect();
        rows.push(ContactRow::Header {
            letter,
            count: group.len(),
        });
        for (i, &&(first, last, note)) in group.iter().enumerate() {
            rows.push(ContactRow::Person(Person {
                first,
                last,
                note,
                tone: TONES[(rows.len() + i) % (TONES.len() - 1)],
                end: i + 1 == group.len(),
            }));
        }
    }
    rows
}

/// A letter's header row, of the cell's `row`.
fn contact_header(row: Memo<ContactRow>) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(px(34.0))
        .px_4()
        .gap(px(10.0))
        // Opaque: rows scroll under it while it sticks.
        .bg(colors::BACKGROUND)
        .border_b_1()
        .border_b_color(colors::BORDER)
        .child(
            text(move || row.get().header().0.to_string())
                .text_size(px(13.0))
                .font_weight(FontWeight::BOLD)
                .text_color(colors::ACCENT),
        )
        .child(view().flex_1().h(px(1.0)).bg(colors::LINE))
        .child(
            text(move || row.get().header().1.to_string())
                .text_size(px(13.0))
                .text_color(colors::FAINT)
                .tabular_nums(),
        )
}

/// A person's row, of the cell's `row`.
fn contact_person(row: Memo<ContactRow>) -> impl IntoElement {
    let person = move || row.get().person();
    view()
        // A tap lights the row up and does nothing else.
        .on_press(|_: &()| {})
        .active(|s| s.bg(colors::RAISED))
        .flex_row()
        .items_center()
        .gap_3()
        .pl_4()
        .h(px(60.0))
        .child(
            view()
                .size(px(38.0))
                .rounded(px(19.0))
                .bg(move || person().tone.tint())
                .items_center()
                .justify_center()
                .child(
                    text(move || {
                        let p = person();
                        p.first
                            .chars()
                            .take(1)
                            .chain(p.last.chars().take(1))
                            .collect::<String>()
                    })
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || person().tone.color()),
                ),
        )
        .child(
            view()
                .flex_1()
                .self_stretch()
                .justify_center()
                .gap(px(2.0))
                .pr_4()
                .style_when(
                    move || !person().end,
                    |v| v.border_b_1().border_b_color(colors::LINE),
                )
                .child(
                    text(move || format!("{} ", person().first))
                        .text_size(px(15.0))
                        .text_color(colors::TEXT)
                        .child(text(move || person().last).font_weight(FontWeight::SEMIBOLD)),
                )
                .child(
                    text(move || person().note)
                        .number_of_lines(1)
                        .text_size(px(13.0))
                        .text_color(colors::MUTED),
                ),
        )
}

/// Section headers stick to the top until the next one pushes them out.
#[component]
fn Contacts() -> impl IntoElement {
    let rows = Rc::new(contact_rows());
    // The headers' rows stick.
    let headers: Vec<usize> = (0..rows.len())
        .filter(|&i| matches!(rows[i], ContactRow::Header { .. }))
        .collect();
    let count = rows.len();
    view().flex_1().child(
        virtual_list(count, move |at| {
            // A cell is a header's or a person's as the row it shows is:
            // the kind picks the branch, the rest is bound.
            let rows = rows.clone();
            let row = computed(move || rows[at.index()]);
            let header = computed(move || matches!(row.get(), ContactRow::Header { .. }));
            If::new(header, move || contact_header(row)).otherwise(move || contact_person(row))
        })
        .sticky_header_indices(headers)
        .estimated_item_size(60.0)
        .content_padding_bottom(24.0)
        .flex_1(),
    )
}

/// What the chat's messages say, in turn.
const LINES: [&str; 7] = [
    "Did the build go out?",
    "Yes, 2.4 is live on both stores",
    "Crash rate?",
    "Flat. 0.02 %",
    "Nice. The list scrolls so much smoother now",
    "Recycling plus native layout, no JS per frame",
    "Ship it",
];

/// A message of the chat.
#[derive(Debug, Clone, PartialEq)]
struct Message {
    id: u32,
    text: String,
    mine: bool,
    time: String,
}

/// An inverted list: starts at the newest message, keeps its place as
/// older ones load.
#[component]
fn Chat(cx: Scope<NoEvent>) -> impl IntoElement {
    // The id of the next message.
    let next = stored(0_u32);
    let message = move |text: String, mine: bool| {
        let id = next.with(|n| {
            *n += 1;
            *n - 1
        });
        Message {
            id,
            text,
            mine,
            time: format!("{:02}:{:02}", 9 + (id / 60) % 12, id % 60),
        }
    };
    // Newest first.
    let messages = signal(
        (0..30)
            .map(|i| message(LINES[i % LINES.len()].to_owned(), i % 3 == 0))
            .collect::<Vec<_>>(),
    );
    let draft = signal(String::new());
    // Pages of older messages loaded.
    let older = signal(0_u32);
    let send = move || {
        if draft.with_untracked(|d| d.trim().is_empty()) {
            return;
        }
        animate_layout(LayoutAnimation::SPRING);
        let text = draft.get_untracked();
        draft.set(String::new());
        let sent = message(text, true);
        messages.update(|m| m.insert(0, sent));
    };
    let load_older = move |_: &()| {
        let count = messages.with_untracked(Vec::len);
        older.update(|o| *o += 1);
        let loaded: Vec<_> = (0..20)
            .map(|i| message(format!("Older message {}", count + i + 1), i % 4 == 0))
            .collect();
        messages.update(|m| m.extend(loaded));
    };
    let list = virtual_list(
        move || messages.with(Vec::len),
        move |at| {
            // A cell shows other messages as it is reused: what it shows is
            // bound to its position.
            let mine = computed(move || messages.with(|m| m[at.index()].mine));
            let theirs = move || !mine.get();
            view()
                .style_when(mine, |v| v.self_end().items_end())
                .style_when(theirs, |v| v.self_start().items_start())
                .max_w(relative(0.78))
                .gap(px(3.0))
                .child(
                    view()
                        .px(px(14.0))
                        .py(px(9.0))
                        .rounded(px(18.0))
                        .border_curve(BorderCurve::Continuous)
                        .rounded_br(move || px(if mine.get() { 6.0 } else { 18.0 }))
                        .rounded_bl(move || px(if mine.get() { 18.0 } else { 6.0 }))
                        .style_when(mine, |v| v.bg(colors::ACCENT_FILL).finish(Finish::Accent))
                        .style_when(theirs, |v| {
                            v.bg(colors::RAISED)
                                .finish(Finish::Raised)
                                .border_1()
                                .border_color(colors::BORDER)
                        })
                        .child(
                            text(move || messages.with(|m| m[at.index()].text.clone()))
                                .text_color(move || {
                                    if mine.get() {
                                        colors::ON_ACCENT
                                    } else {
                                        colors::TEXT
                                    }
                                })
                                .text_size(px(15.0)),
                        ),
                )
                .child(
                    text(move || messages.with(|m| m[at.index()].time.clone()))
                        .text_size(px(11.0))
                        .text_color(colors::FAINT)
                        .tabular_nums()
                        .px_1(),
                )
        },
    )
    .keys(move || messages.with(|m| m.iter().map(|m| m.id).collect::<Vec<_>>()))
    .inverted()
    .estimated_item_size(52.0)
    .content_padding_x(16.0)
    .content_padding_y(12.0)
    .row_gap(6.0)
    .flex_1()
    .keyboard_dismiss_mode(KeyboardDismissMode::Interactive)
    .on_end_reached(load_older)
    .end_reached_threshold(0.5);
    let room = view()
        .flex_row()
        .items_center()
        .gap_2()
        .px_4()
        .pb_2()
        .child(view().size(px(8.0)).rounded(px(4.0)).bg(colors::UP))
        .child(
            text("Release room")
                .flex_1()
                .text_size(px(13.0))
                .text_color(colors::MUTED),
        )
        .child(
            text(move || {
                format!(
                    "{} messages, {} older pages",
                    messages.with(Vec::len),
                    older.get()
                )
            })
            .text_size(px(13.0))
            .text_color(colors::FAINT)
            .tabular_nums(),
        );
    // Clear of the home indicator unless the keyboard covers it.
    let bottom = move || {
        if keyboard_height() > 0.0 {
            0.0
        } else {
            cx.insets().bottom
        }
    };
    let bar = view()
        .flex_row()
        .items_center()
        .gap_2()
        .px_4()
        .pt_2()
        .pb(move || px(8.0 + bottom()))
        .bg(colors::SURFACE)
        .border_t_1()
        .border_t_color(colors::BORDER)
        .child(
            input()
                .value(move || draft.get())
                .on_change_text(move |text: &str| draft.set(text.to_owned()))
                .on_submit_editing(move |_: &str| send())
                .blur_on_submit(false)
                .placeholder("Message")
                .placeholder_color(|| color(colors::FAINT))
                .selection_color(|| color(colors::ACCENT))
                .return_key_type(ReturnKeyType::Send)
                .flex_1()
                .h(px(44.0))
                .text_size(px(15.0))
                .text_color(colors::TEXT)
                .bg(colors::INSET)
                .finish(Finish::Inset)
                .border_1()
                .border_color(colors::BORDER)
                .rounded(px(22.0))
                .px_4(),
        )
        .child(RoundButton::new(
            IconName::ArrowUp,
            "Send",
            move |_: &()| send(),
        ));
    view().flex_1().child(
        KeyboardAvoidingView::element(KeyboardAvoidingViewProps::default())
            .flex_1()
            .child(room)
            .child(list)
            .child(bar),
    )
}

/// The ranges of the horizontal scroll view.
const RANGES: [&str; 7] = [
    "Today",
    "Yesterday",
    "This week",
    "This month",
    "This year",
    "All time",
    "Custom",
];

/// An item of the empty list.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    n: usize,
    tone: Tone,
}

/// The pull to refresh section: the spinner's state and the refreshes.
fn refresh_section(refreshing: Signal<bool>, refreshed: Signal<u32>) -> Section {
    Section::new("Pull to refresh").child(
        view()
            .flex_row()
            .items_center()
            .gap_3()
            .child(IconTile::new(IconName::ArrowDown).tint(move || {
                if refreshing.get() {
                    colors::ACCENT
                } else {
                    colors::TEXT
                }
            }))
            .child(
                view()
                    .flex_1()
                    .gap(px(2.0))
                    .child(
                        text(move || {
                            if refreshing.get() {
                                "Refreshing…"
                            } else {
                                "Pull the page down"
                            }
                        })
                        .text_size(px(15.0))
                        .text_color(colors::TEXT),
                    )
                    .child(
                        text("The system spinner on a plain scroll view")
                            .text_size(px(13.0))
                            .text_color(colors::MUTED),
                    ),
            )
            .child(
                view()
                    .items_end()
                    .child(
                        text(move || refreshed.get().to_string())
                            .text_size(px(28.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors::TEXT)
                            .tabular_nums(),
                    )
                    .child(
                        text("refreshes")
                            .text_size(px(12.0))
                            .text_color(colors::FAINT),
                    ),
            ),
    )
}

/// The horizontal scroll view of range chips, `range` the selected one.
fn ranges(range: Signal<&'static str>) -> Section {
    let chips = RANGES.map(|label| {
        let selected = computed(move || range.get() == label);
        press_with(
            PressProps {
                hit_slop: Some(4.0),
                role: PressRole::Tab.into(),
                selected: selected.into(),
                label: Some(label.into()).into(),
                ..PressProps::default()
            },
            move |_: &()| range.set(label),
        )
        .child(
            view()
                .h(px(36.0))
                .px(px(14.0))
                .rounded(px(18.0))
                .justify_center()
                .style_when(selected, |v| v.bg(colors::LIGHT).finish(Finish::Light))
                .style_when(
                    move || !selected.get(),
                    |v| {
                        v.bg(colors::RAISED)
                            .finish(Finish::Raised)
                            .border_1()
                            .border_color(colors::BORDER)
                    },
                )
                .child(
                    text(label)
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || {
                            if selected.get() {
                                colors::ON_LIGHT
                            } else {
                                colors::MUTED
                            }
                        }),
                ),
        )
    });
    Section::new("Horizontal scroll view")
        .note(move || range.get())
        .child(
            view().mx(px(-14.0)).child(
                scroll()
                    .horizontal()
                    .shows_indicator(false)
                    .child(view().flex_row().gap_2().px(px(14.0)).children(chips)),
            ),
        )
}

/// A thousand recycled cards in a horizontal list.
fn cards() -> Section {
    // A cell shows other cards as it is reused: what differs between cards
    // is bound to its position.
    let cards = virtual_list(1000, |at| {
        let tone = move || TONES[at.index() % (TONES.len() - 1)];
        let load = move || ((at.index() * 37) % 100) as f32 / 100.0;
        view()
            .w(px(120.0))
            .h(px(124.0))
            .rounded(px(radius::INSET))
            .border_curve(BorderCurve::Continuous)
            .bg(colors::RAISED)
            .finish(Finish::Raised)
            .border_1()
            .border_color(colors::BORDER)
            .p_3()
            .justify_between()
            .child(
                text(move || format!("Card {}", at.index() + 1))
                    .text_size(px(12.0))
                    .text_color(colors::MUTED)
                    .tabular_nums(),
            )
            .child(
                text(move || ((load() * 100.0).round() as u32).to_string())
                    .text_size(px(26.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::TEXT)
                    .tabular_nums()
                    .child(
                        text(" %")
                            .text_size(px(15.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || tone().color()),
                    ),
            )
            .child(Meter::new(load).tone(tone))
    })
    .horizontal()
    .estimated_item_size(130.0)
    .shows_indicator(false)
    .content_padding_x(14.0)
    .row_gap(10.0)
    .h(px(124.0));
    Section::new("Horizontal list")
        .note("1,000 recycled cards")
        .child(view().mx(px(-14.0)).child(cards))
}

/// A list that starts empty, with separators; rows are added and cleared.
fn empty_list() -> Section {
    let items = signal(Rc::new(Vec::<Item>::new()));
    let count = computed(move || items.with(|i| i.len()));
    // Changes to the list animate.
    let add = move |_: &()| {
        animate_layout(LayoutAnimation::EASE_IN_EASE_OUT);
        items.update(|items| {
            let items = Rc::make_mut(items);
            items.push(Item {
                n: items.len() + 1,
                tone: TONES[items.len() % (TONES.len() - 1)],
            });
        });
    };
    let clear = move |_: &()| {
        animate_layout(LayoutAnimation::EASE_IN_EASE_OUT);
        items.set(Rc::default());
    };
    let list = virtual_list(count, move |at| {
        // The item this cell shows: a cell shows another item through its
        // bindings.
        let item = computed(move || items.with(|i| i.get(at.index()).cloned()));
        let tone = move || item.with(|i| i.as_ref().map_or(Tone::Neutral, |i| i.tone));
        view()
            .h(px(48.0))
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .px(px(14.0))
            .child(
                view()
                    .size(px(8.0))
                    .rounded(px(2.5))
                    .bg(move || tone().color()),
            )
            .child(
                text(move || {
                    item.with(|i| {
                        i.as_ref()
                            .map(|i| format!("Item {}", i.n))
                            .unwrap_or_default()
                    })
                })
                .flex_1()
                .text_size(px(15.0))
                .text_color(colors::TEXT),
            )
            .child(Badge::new(move || tone_name(tone())).tone(tone))
    })
    .estimated_item_size(48.0)
    .empty(
        view()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .h(px(194.0))
            .child(IconTile::new(IconName::Lists).tint(colors::FAINT))
            .child(
                text("Nothing here yet: tap Add")
                    .text_size(px(13.0))
                    .text_color(colors::MUTED),
            ),
    )
    .separator(|| view().h(px(1.0)).ml(px(14.0)).bg(colors::LINE))
    .h(px(196.0))
    .rounded(px(radius::CONTROL))
    .bg(colors::SURFACE)
    .finish(Finish::Card)
    .border_1()
    .border_color(colors::LINE);
    let mono = |name: &'static str| text(name).font_family(MONO).text_color(colors::TEXT);
    Section::new("An empty list")
        .note(move || format!("{} items", count.get()))
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(
                    view()
                        .flex_1()
                        .child(Button::new("Add", add).icon(IconName::Plus)),
                )
                .child(
                    view().flex_1().child(
                        Button::new("Clear", clear)
                            .variant(ButtonVariant::Secondary)
                            .disabled(Prop::bind(move || count.get() == 0)),
                    ),
                ),
        )
        .child(list)
        .child(
            text("")
                .text_size(px(13.0))
                .text_color(colors::MUTED)
                .child(mono("ListEmptyComponent"))
                .child(" while empty, separators between rows; changes animate with ")
                .child(mono("LayoutAnimation"))
                .child("."),
        )
}

/// Horizontal scrolling, pull to refresh on a plain scroll view, and an
/// empty list.
#[component]
fn More() -> impl IntoElement {
    let refreshing = signal(false);
    let refreshed = signal(0_u32);
    let range = signal("This week");
    // The refresh in progress.
    let refresh = stored(None::<Spawned<()>>);
    let on_refresh = move |_: &()| {
        refreshing.set(true);
        let task = set_timeout(Duration::from_secs(1), move || {
            refreshed.update(|r| *r += 1);
            refreshing.set(false);
        });
        if let Some(old) = refresh.replace(Some(task)) {
            old.cancel();
        }
    };
    view().flex_1().child(
        scroll()
            .flex_1()
            .refreshing(refreshing)
            .on_refresh(on_refresh)
            .child(
                view()
                    .p_4()
                    .gap_4()
                    .child(refresh_section(refreshing, refreshed))
                    .child(ranges(range))
                    .child(cards())
                    .child(empty_list()),
            ),
    )
}

/// The demo's tabs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Grid,
    Sections,
    Chat,
    More,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Grid, "Grid"),
    (Tab::Sections, "Sections"),
    (Tab::Chat, "Chat"),
    (Tab::More, "More"),
];

/// The demo's page: the tabs and the selected one's list.
#[component]
pub(crate) fn Lists() -> impl IntoElement {
    let tab = signal(Tab::default());
    view()
        .flex_1()
        .bg(colors::BACKGROUND)
        .child(
            view().px_4().pt_2().pb_2().child(
                segmented(
                    TABS.map(|t| Segment::new(t.1)),
                    Prop::bind(move || {
                        TABS.iter().position(|t| t.0 == tab.get()).unwrap_or(0) as u32
                    }),
                )
                .on_event(move |e: &SegmentedEvent| {
                    let SegmentedEvent::Change(i) = *e;
                    tab.set(TABS[i as usize].0);
                }),
            ),
        )
        // A tab's list is made when it is chosen, and comes in.
        .child(dynamic(move || {
            let content = match tab.get() {
                Tab::Grid => Grid::element(()).into_element(),
                Tab::Sections => Contacts::element(()).into_element(),
                Tab::Chat => Chat::element(()).into_element(),
                Tab::More => More::element(()).into_element(),
            };
            Appear::element(AppearProps {
                flex: true,
                ..AppearProps::default()
            })
            .child(content)
        }))
}
