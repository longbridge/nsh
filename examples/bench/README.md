# Component benchmarks

The screens of the component benchmark with `TsButton.tsx`, a Button written in TSX (drawn exactly like the Rust
Button of the benchmark's own UI set, `test/ui`, with the default theme):

- `GridTs.tsx`: 200 Buttons in a scroll view; the first press target updates every title.
- `RowsTs.tsx`: a recycled list of 1000 rows with a Button each.

`cargo test --release -p nsh-engine --test engine bench_components -- --ignored --nocapture` prints the JS and core
time of each screen with both Buttons; `nsh test examples/bench/GridTs.tsx --trace` shows the mount items. Results:
`docs/dev/performance.md`.

`Inputs.tsx` holds the input controls' hot paths: a searchable Select of 10 000 items, a TimeField, a NumberInput and
an OtpInput. Steps on standard input drive them, `--trace` shows each keystroke's commit, e.g.
`echo 'await app.press("Pick"); await app.type("Search", "4"); await app.type("text:4", "2")' | nsh test examples/bench/Inputs.tsx --trace`.

`data-table/` is a DataTable of 100 000 rows × 20 columns whose app serves the rows around those in view
(`onVisibleRowsChange`), as an app over more values than a prop takes (a million) must. Its test opens a 1280 × 800
desktop window and holds it for 20 s while something outside drives it: on Linux, Xvfb and XTest dragging the scroll
bar's thumb or turning the wheel, with `nsh test examples/bench/data-table --target linux --fps -v` logging the frame
rate and the engine's cost each second. Results: `docs/dev/performance.md` ("DataTable scroll bar drag").

`RichEditor.tsx` holds 1,000 paragraphs of 80 ASCII characters. Each press alternates inserting and deleting one
character in the first paragraph, retaining the rows' keys and order:

```sh
echo 'for (let i = 0; i < 8; i++) await app.press("id:edit");' | nsh test examples/bench/RichEditor.tsx --trace
```

Compare the final four edits after four warmups. Headless runs keep the complete document mounted; the trace
includes selection geometry as well as the changed text. Results: `docs/dev/performance.md`.

`ChildAppends.tsx` measures ordinary sibling insertion, with `COUNT` selecting the size:

```sh
cargo build -p nsh-cli --locked -j2 --config profile.dev.package.nsh-core.opt-level=3
./target/debug/nsh test examples/bench/ChildAppends.tsx --trace
```

Set `MOUNTED=true` to start with an empty container, then append on a press:
`echo 'await app.press("id:append");' | ./target/debug/nsh test examples/bench/ChildAppends.tsx --trace`.
The Core-only benchmark covers both cases, constructing operations and checking order, frames and full mount-batch
hashes outside its timed interval:

```sh
cargo test -p nsh-core --lib --features declarations --locked -j2 \
  --config profile.dev.package.nsh-core.opt-level=3 bench_child_appends \
  -- --ignored --nocapture --test-threads=1
```

Compare alternating runs of both versions on the same CPU, with no builds running. Results and scope:
`docs/dev/performance.md`.
