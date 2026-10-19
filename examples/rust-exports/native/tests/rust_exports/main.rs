//! `App.tsx` against the crate's set, headless: the TSX entry is bundled
//! and run on QuickJS as `nsh run App.tsx` does, presses go through its
//! UI, and each test checks what one export shows (one test binary: each
//! links the bundler, QuickJS and the engine).

mod common;
mod declarations;

use common::Running;
use nsh_core::Command;

/// What the first frame shows.
const FIRST: &str = "Formatted at once: $1,234.50";

#[test]
fn the_module_returns_values_at_once_and_promises() {
    let mut app = Running::start(FIRST);
    // `Ledger.format` ran inside the TSX call (the text above); `copy` is
    // an op the UI applies in order, `fetchRate` a task.
    app.press("Copy statement", "Copied 32 characters");
    assert!(
        app.commands.contains(&Command::SetClipboard(
            "Savings: $120.00 after 0 entries".into()
        )),
        "the op put the statement on the clipboard: {:?}",
        app.commands
    );
    app.press("Fetch rate", "Rate: 5%");
}

#[test]
fn the_class_keeps_its_state_in_rust() {
    let mut app = Running::start(FIRST);
    // The constructor, the static, the getters, and `using`'s account.
    for shown in [
        "Savings: $120.00",
        "Checking: $40.00",
        "Draft: $15.00 after 1 entry",
        "Heard: nothing yet",
    ] {
        assert!(app.shows(shown), "{shown}");
    }
    // `&mut self`, posting to the listener from inside the call.
    app.press("Deposit $25", "Deposited: $145.00");
    app.wait_for("Heard: 145");
    // Another instance as an argument.
    app.press("Move $30", "Moved $30, $115.00 left");
    app.wait_for("Checking: $70.00");
    app.wait_for("Heard: 145, 115");
    // The async member: the listener hears its task's post, then the
    // Promise resolves (1% without a fetched rate).
    app.press("Accrue", "Accrued: $116.15");
    assert!(app.shows("Heard: 145, 115, 116.15"));
    assert!(app.shows("Savings: $116.15"));
    // The setter, read back through the getter.
    app.press("Rename", "Rainy day: $116.15");
    // A refused deposit throws its `Err`.
    app.press("Move $30", "Moved $30, $86.15 left");
    app.press("Move $30", "Moved $30, $56.15 left");
    app.press("Move $30", "Moved $30, $26.15 left");
    app.press("Move $30", "cannot move $30.00 out of $26.15");
}

#[test]
fn a_disposed_account_throws() {
    let mut app = Running::start(FIRST);
    app.press("Close checking", "Account is disposed");
    assert!(app.shows("Checking: closed"));
    // Using it as an argument throws too, and the other account is left
    // as it was.
    app.press("Deposit $25", "Deposited: $145.00");
    app.press("Move $30", "Account is disposed");
    assert!(app.shows("Savings: $145.00"));
}

#[test]
fn the_component_takes_props_children_and_reports_its_event() {
    let mut app = Running::start(FIRST);
    // `value` from TSX, `max` defaulted to 100, `label` given, the
    // children placed under the bar.
    for shown in [
        "Groceries",
        "25%",
        "$25.00 of $100.00",
        "Pressed 0×, onChange gave 25",
    ] {
        assert!(app.shows(shown), "{shown}");
    }
    // Each press asks for `step` (25) more through `onChange`, which sets
    // `value` again; past `max` it starts over.
    app.press("Groceries", "Pressed 1×, onChange gave 50");
    assert!(app.shows("50%") && app.shows("$50.00 of $100.00"));
    app.press("Groceries", "Pressed 2×, onChange gave 75");
    app.press("Groceries", "Pressed 3×, onChange gave 100");
    assert!(app.shows("100%"));
    app.press("Groceries", "Pressed 4×, onChange gave 0");
    assert!(app.shows("0%"));
}
