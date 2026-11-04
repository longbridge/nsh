//! A `DiffView` in a terminal: a drag over its code selects the text of
//! one side across its rows, leaving the line numbers and the other side
//! of a split view out, and Ctrl+Shift+C copies it.

use std::sync::Arc;

use nsh_base::diff_view;
use nsh_core::{
    clipboard::{ClipboardBackend, MemoryClipboard},
    prelude::*,
};
use nsh_platform_tui::{
    App, AppOptions, AppView, Input, Modifiers, MouseButton::Left, MouseKind, Root,
};
use ratatui_core::{backend::TestBackend, style::Modifier, terminal::Terminal};

use super::app::{draw_until, mouse};

#[component]
fn Changes() -> impl IntoElement {
    view().flex_1().child(
        diff_view()
            .old_value(SharedString::from("alpha\nbeta\ngamma\n"))
            .new_value(SharedString::from("alpha\nBETA\ngamma\n"))
            .flex_1(),
    )
}

/// The Cell where the `nth` (from 0) `word` on screen starts.
fn find(terminal: &Terminal<TestBackend>, word: &str, nth: usize) -> (u16, u16) {
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width;
    let mut found = Vec::new();
    for y in 0..buffer.area.height {
        let row: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
        let mut from = 0;
        while let Some(at) = row[from..].find(word) {
            let column = row[..from + at].chars().count() as u16;
            found.push((column, y));
            from += at + word.len();
        }
    }
    found[nth]
}

fn reversed(terminal: &Terminal<TestBackend>, (x, y): (u16, u16)) -> bool {
    terminal.backend().buffer()[(x, y)]
        .modifier
        .contains(Modifier::REVERSED)
}

#[test]
fn a_drag_selects_one_sides_code_across_rows_and_copies_it() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut options = AppOptions::default();
    options.clipboard = Some(clipboard.clone());
    options.cache = nsh_platform_tui::CacheOptions::off();
    options.wake = Some(Arc::new(move || {
        let _ = tx.send(());
    }));
    let mut view = AppView::new(options);
    let mut terminal = Terminal::new(TestBackend::new(60, 6)).unwrap();
    view.load(App::Rust(Root::new::<Changes>(|| ()))).unwrap();
    draw_until(&mut view, &mut terminal, &rx, "BETA");

    let start = find(&terminal, "alpha", 0);
    let beta = find(&terminal, "beta", 0);
    let new_gamma = find(&terminal, "gamma", 1);
    let other = find(&terminal, "BETA", 0);
    assert!(start.0 < other.0, "the old side is on the left");
    // From the old side's first line to the new side's last: the drag
    // stays on the side it began on.
    view.handle(mouse(MouseKind::Down(Left), start.0, start.1));
    view.handle(mouse(MouseKind::Move, new_gamma.0 + 4, new_gamma.1));
    view.handle(mouse(MouseKind::Up(Left), new_gamma.0 + 4, new_gamma.1));
    terminal
        .draw(|frame| view.render(frame, frame.area()))
        .unwrap();
    assert!(reversed(&terminal, start));
    assert!(reversed(&terminal, beta));
    // Not the other side's code, nor the line numbers before the code.
    assert!(!reversed(&terminal, other));
    assert!(!reversed(&terminal, (start.0 - 2, start.1)));

    view.handle(Input::Key {
        key: "c".into(),
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        release: false,
    });
    assert_eq!(clipboard.text().as_deref(), Some("alpha\nbeta\ngamma"));
}
