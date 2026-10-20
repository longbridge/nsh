# Component benchmarks

The screens of the component benchmark with `TsButton.tsx`, a Button written in TSX (drawn exactly like the Rust
Button of the benchmark's own UI set, `test/ui`, with the default theme):

- `GridTs.tsx`: 200 Buttons in a scroll view; the first press target updates every title.
- `RowsTs.tsx`: a recycled list of 1000 rows with a Button each.

`cargo test --release -p nsh-engine --test engine bench_components -- --ignored --nocapture` prints the JS and core
time of each screen with both Buttons; `nsh test examples/bench/GridTs.tsx --trace` shows the mount items. Results:
`docs/dev/performance.md`.
