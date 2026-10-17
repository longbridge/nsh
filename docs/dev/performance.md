# Performance

Performance is the project's first priority; the rules are in `AGENTS.md`. Changes to hot paths come with before /
after `nsh trace` timings and item counts. Current baselines (Apple Silicon):

- counter, headless (debug): first frame ~0.5–0.65ms (58 ops / 49 mount items), one tap ~0.15ms (12 ops / 8 items).
- counter, Catalyst (release): first frame ~1.05–1.15ms (of which ~0.7ms is cold CoreText layout of 10 texts), one tap ~0.12ms.
- Hot reload: ~120ms from saving a file to the device running the new version (file event latency and ~60ms of development
  bundling included); applying a hot update takes 0.2–0.5ms on the engine thread.
- gallery Photos list (`examples/gallery/Bench.tsx`, debug, headless): first frame 653 ops / 224 nodes, ~2.7ms (list bind
  rounds included).
- Photos scrolling (release): a frame that rebinds one row is 4 ops / 4 mount items; frames that reveal no new row call no JS and produce
  no batch. `list_bench` runs at ~9µs per frame back to back (JS ~4µs; ~30µs / JS ~20µs before reactivity went native) and
  ~50µs with caches flushed between frames, but ~60–150µs after the thread sleeps 10ms. The difference is the CPU clocking down while the thread sleeps (Apple CLPC), not the code path; neither QoS (the engine
  thread is USER_INTERACTIVE) nor the Mach time-constraint policy changes it. Judge optimizations by the back-to-back and flushed-cache numbers:
  single-frame `nsh trace` timings are dominated by the clock state.
- On Catalyst (release), rebinding one row while scrolling slowly takes ~0.4ms (clocked down) and rebinding a whole screen of ~32 rows on a
  fast fling 0.9–1.6ms.
- Host startup (`Host::new`, release): ~75–155µs (clock state). The web APIs add nothing until first used, then ~90µs to
  make their classes (`bench_first_web_global`; as JS they cost ~0.8ms as source, ~0.33ms as bytecode).
- Android emulator (arm64, release): rebinding one gallery row while scrolling ~0.3–0.4ms (JS ~60µs); text costs per new
  text: `PrecomputedText` ~120µs, measure ~50µs, final layout reused. First gallery frame 20–50ms (noisy on the emulator).

- Rust components vs the same component in TSX (`component_bench`, `examples/bench`, release, minimum of the runs;
  `TsButton.tsx` is the TSX Button `nsh/ui/mobile` had before components moved into Rust, restyled to draw exactly
  like the Rust one; measured on the former JS runtime):

  | | Rust Button: JS / core | TSX Button: JS / core |
  |---|---|---|
  | first frame, 200 Buttons (ops) | 1.32ms / 0.77ms (1015) | 2.32ms / 0.70ms (2415) |
  | one update of all 200 titles (ops) | 0.42ms / 0.42ms (200) | 0.77ms / 0.36ms (600) |
  | scrolling a 1000-row list, per rebinding frame | 8.3µs / 5.9µs | 10.2µs / 5.4µs |

  JS does half the work or less, a net win of ~30% on the first frame and ~25% on updates. The core's extra time is
  not the component code (its renders and diffs are ~15% of a commit) but the allocator: Taffy's compute takes the
  same samples in both, while its per-layout vectors spend 2.5× longer in `malloc` / `free` / `madvise` after the
  render trees the components build and drop each update (libmalloc coalesces the freed blocks and returns pages).
  Pooling the style buffers bought 3%; a different global allocator in the binaries is the lever left. The
  components' JS face must stay one effect per instance reading the props directly: the first version (Solid's
  `splitProps` / `mergeProps` / `spread` in the former JS runtime) cost 4.0ms of JS for the same first frame, more
  than the TSX Button.
- Canvas recording (release `nsh trace`, minimum of the runs): in the former JS runtime, 2500 iterations of
  `beginPath`, `moveTo`, `lineTo`, `lineWidth =`, `strokeStyle =`, `stroke`, `fillRect` (17 500 calls) recorded into a
  JS array in ~1.4ms, the same with methods generated from the specs as with a hand-written class (an A/B of the two
  mechanisms in one QuickJS run: 4.16 vs 4.22ms, medians within 1%, for 80 000 calls).
  Decoding that drawing in the core (`nsh_core::js::canvas::decode`, from the QuickJS array) takes ~0.52–0.57ms
  (`cargo test --release -p nsh-js --lib bench_canvas_decode -- --ignored --nocapture`), as the hand-written decoder
  it replaced did (0.57ms).
  The native context records into a Rust list (`nsh-js` `canvas.rs`): the same 17 500 calls take ~1.0–1.1ms
  (~1.7ms in the former JS runtime, measured A/B), and 12 of them plus decoding one commit 21–22ms (61ms).

- Where JS time went in the former JS runtime (release `nsh trace`, which logs `event js` per dispatched event;
  2026-10-16, before the runtime went native): with Solid in JS, the empty app's eval was ~250µs (all of it the
  runtime and Solid starting), counter's ~380µs (first frame ~500µs), and a counter tap spent a median 52µs in JS
  (handler, Solid's updates, the microtask flush and the op decode) against 17µs of core commit (11 ops, 10 items).
  The native reactive graph (`nsh-js` `reactive.rs`, still under the JS runtime), measured A/B on the same loaded
  machine: empty eval ~210µs vs ~300µs, counter eval ~330µs
  vs ~460µs, counter tap median ~47µs vs ~69µs in JS, gallery list rebind (`list_bench`) JS ~4µs vs ~21µs per frame.
- The native runtime (`native-pipeline.md`) against the former JS runtime, release, the same counter app (a counter,
  `<If>`, a 3-row `<Each>`) on both: bundle 886 B JS / 1.7 KB bytecode vs 14 KB / 19.6 KB, eval ~95µs vs ~390µs, tap
  median ~5µs vs ~14µs in JS, commit ~15µs vs ~22µs. Scrolling a 1000-row `List` (avatar, two texts, a bound color)
  6000 pt: ~77µs vs ~150µs of JS per rebinding frame, commit ~205µs vs ~276µs.

## The performance monitor (`nsh_engine::perf`)

`PerfMonitor` collects one surface's timings for an on-screen monitor (`--fps`; the macOS backend's `fps.rs` draws it)
or an app's own tooling (`AppView::perf_monitor`), modeled on gpui-kit's `gpui-fps`. The engine thread records each
wake-up that committed (`CommitTiming`: the core's apply / render / layout / diff, and the rest as the app's code) once
`EngineControl::set_perf_monitor` installs one; the platform records its main-thread work (`record_work`), closes a
frame at each display refresh (`vsync`: a refresh in which something changed is a shown frame) and the batches'
engine-to-main-thread latency. `snapshot()` gives the rates over the last second, the distributions over the last 120
frames / commits (mean, p95, max, the share over one refresh), the engine's busy share and the process' CPU and memory
(`ProcessSampler`: `getrusage`, the physical footprint on Apple platforms, the resident set on Linux / Android). The
first frames and commits are cold and not counted. Without a monitor nothing is timed (one `Option` per wake-up and
per frame). Other platforms need only call `record_work` / `vsync` / `record_latency` where the macOS surface does and
draw a panel.

- macOS (release, Apple silicon, 1200×800 window, 120 Hz ProMotion screen; `nsh-macos-host --fps --trace` with
  `--script`, two alternating runs of each; main-thread ms per shown frame as p50 / p95). The display link delivers
  120 Hz in every case (refresh gaps p50 8.33 ms). Before = the backend without the two drawing fixes below:

  | Scenario | Before: main p50 / p95, CPU | After: main p50 / p95, CPU |
  |---|---|---|
  | Stocks, idle (a tick every 50 ms, pulsing canvases) | 3.2–3.5 / 5.9–6.8 ms, 59–67% | 2.5–2.7 / 5.2–5.6 ms, 47–55% |
  | Stocks, scrolling the watchlist | 3.5–3.6 / 7.5–7.7 ms, 64–69% | 3.0–3.4 / 6.3–6.7 ms, 59–64% |
  | Lists grid, scrolling | 0.4–0.6 / 6.3–6.4 ms, 33–34% | 0.3–0.6 / 2.0–5.9 ms, 14–32% |
  | Animations demo | 0.2–0.5 / 0.2–0.8 ms | 0.1–0.3 / 0.2–0.6 ms |

  Single runs move by ~20% (other processes); earlier runs of Stocks without the fixes read p95 ~10.7 ms with 8–12%
  of frames over 8.3 ms. Sampled (`sample`), the main thread on Stocks is ~85% busy before and after, ~75% of it
  Core Animation drawing layers (`drawRect:` recorded ~18% → ~11%, the backing store replay the rest): the demo
  redraws its 524×340 chart canvas on every frame for a pulsing dot. Per Stocks tick the engine applies ~230 ops
  into 60–110 mount items in ~0.8–1.2 ms (`nsh trace --width 1200 --height 800 --wait 300 --duration 2000`).
  The fixes: drawing views clip to their bounds (AppKit redrew every text of a scroll view on every scroll step,
  ~150 per wheel event in Lists, ~3.5 ms each event in a debug build), and canvases out of sight are not redrawn by
  their animations.

  Partial canvas redraws (`nsh_core::canvas::DamageScan`, `docs/dev/canvas.md`): an animation of a canvas's native
  values redraws only what the operations reading them cover (Stocks: the chart's ring ~31×31 pt of its 524×340, the
  sparklines' ~23×25 of 124×58; whole only on the ticks). With `drawRect:` this halved the main-thread frame (Stocks
  idle `main` p50 ~3.0 → ~2.0 ms, CPU ~62% → ~44%). On top of content layers drawn asynchronously the gain is within
  noise (four runs each, ABBA, presented frames by screen capture): Stocks idle window updates 75–96 /s both, `main`
  p50 ~1.9 → ~1.7 ms, CPU ~38–40% both; a test canvas (524×340: a 400-point area and line, 400 bars, a pulsing ring,
  a moving square) presented 86–117 fps before and 92–119 after, `main` p50 ~1.2 → ~1.0 ms, CPU ~17% both.

- GPUI backend (`nsh-platform-gpui`; release with thin LTO, rootful Xwayland at scale 2, a focused 285×315 pt window at
  60 Hz, against the GTK backend (`GSK_RENDERER=ngl`) on the same display and bundle). `draw` is the surface element's
  prepaint and paint (the whole view tree, every frame), `frame` GPUI's whole frame with presenting (`ZED_MEASUREMENTS=1`),
  CPU the process' (both threads):

  | Scenario | GPUI: draw p50 / p95, frame p50 / p95, CPU | GTK: CPU |
  |---|---|---|
  | Photos list, three mouse flings (61 fps while gliding) | 0.29 / 0.42 ms, 0.63 / 0.89 ms, 4% | 7% |
  | Photos list, wheel notches every 30 ms | 0.23 / 0.30 ms, 0.60 / 0.87 ms, 3–4% | 4% |
  | Animations demo (three native loops) | 0.24 / 0.29 ms, 0.61 / 0.97 ms, 4% | 4% |

  An idle surface draws no frames (frames are requested only while something animates or JS asked for one). GPUI caps
  unfocused windows at 30 fps by default (`WindowOptions::inactive_frame_interval`), skipping frames in between (gaps of
  33–50 ms): nsh's own windows turn the cap off (61 fps unfocused); hosts embedding an `AppView` decide for theirs.
  Debug builds draw the Animations page in 1.5–3.5 ms.

  On the Wayland desktop (Hyprland, 3840×2160 at 144 Hz, scale 1.6; a 1500×900 pt window, unfocused, release with
  thin LTO, `nsh-gpui-host --fps` with `ZED_MEASUREMENTS=1` (`gpui.md` "Frame statistics"); `paint` is the main
  thread's per frame, `frame` GPUI's draw and present):

  | Demo | Before: fps, paint p50, frame p50 | After: fps, paint p50, frame p50 |
  |---|---|---|
  | Animations (three native loops, ~26 paths a frame) | 26, 0.62 ms, 1.07 ms | 145, 0.42 ms, 0.76 ms |
  | Stocks (live prices, sparklines, chart) | 25, 18.4 ms, 19.3 ms | 143, 1.9 ms, 2.5 ms |
  | Gestures (springs, a pulsing ring) | 26, 0.21 ms, 0.52 ms | 145, 0.17 ms, 0.35 ms |

  Before: GPUI's 30 fps cap of unfocused windows (frames 40–42 ms apart) and, in Stocks, canvases rasterized in full
  on the main thread (18 ms of every frame: ~190 new drawings a second). After: no cap in nsh's windows, canvases
  rasterized on two workers and only what animates in place (`canvas.rs`).

The dev profile builds `rquickjs-sys` and `taffy` with `opt-level = 3`, so debug timings are meaningful too; release uses fat LTO.
