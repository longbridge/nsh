# Component benchmarks

The same screens with the Rust `Button` of `nsh/ui/mobile` and with `TsButton.tsx`, the TSX Button it replaced
(restyled to draw exactly like the Rust one with the default theme):

- `GridRust.tsx` / `GridTs.tsx`: 200 Buttons in a scroll view; the first press target updates every title.
- `RowsRust.tsx` / `RowsTs.tsx`: a recycled list of 1000 rows with a Button each.

`cargo test --release -p nsh-engine --test engine bench_components -- --ignored --nocapture` prints the JS and core
time of each; `nsh test examples/bench/GridRust.tsx --trace` shows the mount items. Results: `docs/dev/performance.md`.
