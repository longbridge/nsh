//! The TypeScript declarations `nsh` writes for the crate's set
//! (`.nsh/types/rust-exports/native.d.ts`), as TSX reads them.

/// The declarations of the crate's set.
fn declarations() -> String {
    nsh_core::ui::declarations(&rust_exports_native::__nsh_native_module(), &[])
}

#[test]
fn callbacks_and_events_name_their_arguments() {
    let ts = declarations();
    for line in [
        // `#[args(balance)]` on the parameter.
        "  onChange(listener: (balance: number) => void): void;",
        // An event's data without `#[args]`: `value`.
        "  onChange?: (value: number) => void;",
    ] {
        assert!(ts.contains(line), "missing {line:?} in\n{ts}");
    }
}
