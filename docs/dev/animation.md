# Animation

- **Animation**: `createAnimatedValue()` → `timing` / `spring` / `decay` (return a Promise), `interpolate` (numbers, colors or
  `"deg"` / `"rad"` angles; `extrapolate` / `extrapolateLeft` / `extrapolateRight`: `clamp` (default) / `extend` / `identity`;
  `easing`), `add/sub/mul/div/mod`, `diffClamp(min, max)`. `Easing` (React Native's: `linear`, `ease`, `quad`, `cubic`, `poly`,
  `sin`, `circle`, `exp`, `elastic`, `back`, `bounce`, `bezier`, `in` / `out` / `inOut`) is data decoded in Rust
  (`nsh_animation::Easing::Curve`), never a JS function: the curves, their parameters and defaults are one table
  (`nsh_core::js::animation`), which decodes them, declares their types (`animation::declarations`) and gives `nsh/animated` its curves (`animation::tables`). `Animated.timing` / `spring` / `decay` / `sequence` / `parallel` / `stagger` /
  `delay` / `loop` build a `Composite` tree that runs entirely natively (`AnimationOp::StartComposite`): each step starts at the exact end
  time of the previous one (no frame gaps), `stop()` / `reset()` (back to the values at start) / `start(cb)` report `finished` once
  for the whole tree; leaf animations get internal ids (`LEAF_IDS`) that never reach JS. Composites are released when JS
  lets go of them (their finalizers queue `ReleaseComposite`); a run goes on after that. A run does not keep its
  composite (or the values it animates) alive: `nsh-js` remembers live composites by key (`Animations::composites`),
  not in the run's promise, so an endless `Animated.loop` whose values its component released stops with them (a step on
  a released value ends the run unfinished, rather than counting as done, which made loops spin). Rust components build
  the same trees with `nsh_core::ui::Animation` (composite keys odd, JS's even).
  `decay` takes `clamp: [min, max]` and `bounce`: at a bound it stops, or a new segment of the closed form starts from the
  bound at the exact time it got there with `-bounce` of its speed (frame-rate independent, several per frame if need be).
  Values have an offset (`setOffset` / `flattenOffset` / `extractOffset`, React Native's): they evaluate to value +
  offset; drivers, `set` and gestures move the value. JS reads a node back with `getValue()` (and `stop()`'s promise):
  `AnimationOp::Read` answers with a `Finished` carrying `value`, through the same path as animation ends (Rust
  components: `AnimatedValue::value()`, its request id odd, answered in `Core::animations_finished`).
  `Easing.sample`, `Animated.sampleSpring` / `sampleDecay` return the curves from `nsh_animation::sample_easing` /
  `sample_driver` (the drivers' own code, stepped like frames, at most `MAX_SAMPLES`), so drawn curves are what runs;
  Rust components call `ui::sample_easing` / `sample_spring` / `sample_decay` / `sample_decay_within`.
  Bindable: `opacity`, `backgroundColor`, `borderColor` (all sides), a text's `color` (the whole text: iOS and Android draw
  it into a layer and paint over it in the color, `SourceIn` / `SRC_IN`), `shadowColor` / `shadowOpacity` /
  `shadowRadius`, and the translate / scale / rotate parts of `transform`. iOS shapes the decoration with the bound
  values (and the layers for them even while they are transparent: `bound_style`), then sets the colors and shadow
  on those layers each frame; Android and web get them as properties (`Mounter.java` codes 11–14; the web composes
  `box-shadow`, dropping layered `boxShadow`s while bound). Driven on the main thread by `CADisplayLink`,
  paused while nothing animates. `scrollValue` writes the scroll offset into an animated value in the same frame; `pan` lets drags drive animated values
  directly, and JS receives the velocity when the drag ends. Animated values referenced by a view's style (or `pan`, `scrollValue`, ...) are kept by its node
  (`nsh-js` `dom.rs`) and derived nodes hold their inputs, so bindings do not break when JS objects are collected. In
  Rust components `AnimatedValue` / `AnimatedNode` are `Copy` handles their owner keeps; the element's bindings, a
  drawing or a composite hold the node while bound (`docs/dev/components.md`, "Animated values").
- **Layout animation**: `LayoutAnimation.configureNext(config)` (or the `easeInEaseOut` / `linear` / `spring` presets) animates
  the next commit's frames, insertions and removals natively. The presets are `LayoutAnimation::EASE_IN_EASE_OUT` /
  `LINEAR` / `SPRING` in Rust (what Rust components pass to `animate_layout`), handed to `nsh/animated` as data; the config's
  decoding and types come from the same module.
- **Web**: animated values run in Rust as elsewhere; layout animations, transitions, keyframes and screen transitions
  are Web Animations and CSS made to match the Linux backend (`web.md` "Animation").
