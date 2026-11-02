import type { JSX } from "nsh";
import { Canvas, cleanup, createAnimatedValue, Easing, effect, LayoutAnimation, List, reduceMotion, signal, Text, View } from "nsh";
import { Arc, Area, Bar, Grid, Line, Pie, Plot, PlotAppearScope, PlotAxis, PlotLabel, RadialGrid, RadialLine, Rule, Sankey } from "nsh/base/plot";
import { platform } from "nsh";
import { alpha, Button, Card, chart, colors, DemoPage, finish, focusRing, Legend, mono, Segmented, toneColor, type Tone } from "../ui";

const pointer = platform.desktop;
const size = pointer ? { gap: 10, switch: "small" as const } : { gap: 12, switch: "regular" as const };

/** A repeatable stream of numbers in [0, 1). */
function random(seed: number) {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return (s % 100000) / 100000;
  };
}

/** Values on the axes: small, monospaced and faint, so the marks lead. */
const axis = { fontSize: 10.5, fontFamily: mono, color: colors.faint, fontVariant: ["tabular-nums" as const] };
/** An axis' labels under a plot: a line of `axis` text 8 below its edge. */
const below = { ...axis, lineHeight: 13, paddingTop: 8 };
/** The room under a plot for those labels: its margin. */
const BELOW = 8 + 13;
/** A tooltip's title: a small monospaced caps label over its values. */
const tipTitle = { fontSize: 10, fontFamily: mono, letterSpacing: 0.8, color: colors.muted };
/** A floating tooltip card. */
const tipCard = { ...finish.menu, borderWidth: 1, borderRadius: 10, paddingHorizontal: 10, paddingVertical: 8, gap: 6 };
/** A wash under a line: `color` at `top` opacity fading out to the baseline. */
const wash = (color: string, top = 0.18) => ({
  backgroundImage: `linear-gradient(to bottom, ${alpha(color, top)}, ${alpha(color, 0)})`,
});
const fixed = (n: number, digits = 2) => n.toFixed(digits);
const count = (n: number) => Math.round(n).toLocaleString("en-US");
const signed = (n: number, digits = 2) => `${n >= 0 ? "+" : "−"}${Math.abs(n).toFixed(digits)}`;
/** Grid values at a "nice" step (1, 2 or 5 × 10^k) strictly inside `lo..hi`, about `want` of them. */
function ticks(lo: number, hi: number, want = 4) {
  const raw = (hi - lo) / want;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let v = Math.ceil(lo / step) * step; v < hi; v += step) if (v > lo) out.push(Math.round(v / step) * step);
  return out;
}

/** A label and its value, in a row of figures under a chart. */
function Figure(props: { label: string; value: string; tone?: Tone }) {
  return (
    <View style={{ flex: 1, gap: 4 }}>
      <Text style={{ fontSize: 10, fontFamily: mono, letterSpacing: 0.8, color: colors.faint }}>{props.label.toUpperCase()}</Text>
      <Text style={{ fontSize: 14, fontWeight: "500", color: props.tone ? toneColor(props.tone) : colors.text, fontVariant: ["tabular-nums"] }}>{props.value}</Text>
    </View>
  );
}

/** Figures in a row, over a hairline. */
function Figures(props: { children: JSX.Element }) {
  return <View style={{ flexDirection: "row", gap: 12, paddingTop: 12, borderTopWidth: 1, borderColor: colors.line }}>{props.children}</View>;
}

const LIVE_POINTS = 60;
/** The ticks the live price's channel spans: each tick's high and low of the last ten. */
const CHANNEL = 10;
const LIVE_HEIGHT = 176;
/** The gutter right of the live plot: the y labels and the last price's tag. */
const GUTTER = 58;
/** The window's seconds, under its five ticks from the first row to the last. */
const SECONDS = ["−60s", "−45s", "−30s", "−15s", "now"];
/** The session's open. */
const OPEN = 41.2;
/** The first tick's time of day, in seconds. */
const START = 10 * 3600 + 42 * 60;
/** How a re-fit of the live plot's y domain glides: its marks and labels over `motion.update`, the newest dot with the layout. */
const REFIT = { duration: 450, easing: "easeInOut" } as const;
const refitLayout = {
  duration: REFIT.duration,
  update: { type: "easeInEaseOut" },
  create: { type: "easeInEaseOut", property: "opacity" },
  delete: { type: "easeInEaseOut", property: "opacity" },
} as const;
const clock = (t: number) => {
  const s = START + t;
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(Math.floor(s / 3600))}:${two(Math.floor(s / 60) % 60)}:${two(s % 60)}`;
};

/**
 * A minute of a price, a tick a second: the newest point is the plot's
 * right edge, its dot breathing while the stream runs, its price tagged in
 * the gutter at its height and the window's high written over its point
 * (a `PlotLabel` each, placed by the plot's scales), inside a channel of
 * each tick's high and low over the last ten (an area whose `y0` is a
 * field of each row: a band).
 * The y domain holds still while the window fits in it and re-fits only
 * when the price leaves it or uses too little of it, so the grid does not
 * jump with every tick; a re-fit glides, the line, the band, the grid,
 * the axis' labels and the two `PlotLabel`s on the plot's
 * `motion.update`, the newest dot with a layout animation of the same
 * length.
 */
function Live() {
  const next = random(29);
  let t = 0;
  let price = OPEN;
  const recent: number[] = [];
  const step = () => {
    price = Math.max(1, price + (next() - 0.485) * 0.16);
    t += 1;
    const rounded = Math.round(price * 100) / 100;
    recent.push(rounded);
    if (recent.length > CHANNEL) recent.shift();
    return { t, price: rounded, high: Math.max(...recent), low: Math.min(...recent) };
  };
  const points = signal(Array.from({ length: LIVE_POINTS }, step));
  const fit = (rows: { high: number; low: number }[]): [number, number] => {
    const [lo, hi] = [Math.min(...rows.map((r) => r.low)), Math.max(...rows.map((r) => r.high))];
    const pad = Math.max(0.2, (hi - lo) * 0.3);
    return [Math.floor((lo - pad) * 10) / 10, Math.ceil((hi + pad) * 10) / 10];
  };
  const domain = signal(fit(points()));
  const running = signal(true);
  const hover = signal<number | null>(null);
  const tip = signal(LIVE_POINTS - 1);
  const timer = setInterval(() => {
    if (!running()) return;
    const rows = [...points().slice(1), step()];
    const [lo, hi] = domain();
    const [min, max] = [Math.min(...rows.map((r) => r.low)), Math.max(...rows.map((r) => r.high))];
    const refit = min < lo || max > hi || max - min < (hi - lo) * 0.3;
    if (refit && !reduceMotion()) LayoutAnimation.configureNext(refitLayout);
    points.set(rows);
    if (refit) domain.set(fit(rows));
  }, 1000);
  cleanup(() => clearInterval(timer));

  const last = () => points()[points().length - 1].price;
  const change = () => last() - OPEN;
  const tone = (): Tone => (change() >= 0 ? "up" : "down");
  const y = (v: number) => (1 - (v - domain()[0]) / (domain()[1] - domain()[0])) * LIVE_HEIGHT;
  const grid = () => ticks(domain()[0], domain()[1]);
  const high = () => Math.max(...points().map((p) => p.price));
  const low = () => Math.min(...points().map((p) => p.price));
  /** The row of the window's high (its first, if it repeats). */
  const peak = () => points().findIndex((p) => p.price === high());
  /** The high's text centered over it, kept off the plot's ends. */
  const peakAlign = (): "left" | "center" | "right" => (peak() < 8 ? "left" : peak() > LIVE_POINTS - 9 ? "right" : "center");
  const breathing = () => running() && !reduceMotion();
  return (
    <Card icon="stocks" title="Live price" subtitle="A tick a second in its ten-second range, the last price tagged at its height">
      <View style={{ flexDirection: "row", alignItems: "flex-start" }}>
        <View style={{ flex: 1, gap: 6 }}>
          <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            <Text style={{ fontSize: 13, fontFamily: mono, color: colors.muted }}>NSHL</Text>
            {/* Live: a dot that breathes while the stream runs. */}
            <View style={{ flexDirection: "row", alignItems: "center", gap: 5 }}>
              <View style={{ width: 6, height: 6 }}>
                <If when={breathing()}>
                  <View class="animate-ping" style={{ position: "absolute", width: 6, height: 6, borderRadius: 3, backgroundColor: toneColor("up") }} />
                </If>
                <View style={{ position: "absolute", width: 6, height: 6, borderRadius: 3, backgroundColor: running() ? toneColor("up") : colors.faint }} />
              </View>
              <Text style={{ fontSize: 10, fontFamily: mono, letterSpacing: 0.8, color: running() ? toneColor("up") : colors.faint }}>{running() ? "LIVE" : "PAUSED"}</Text>
            </View>
          </View>
          <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
            <Text style={{ fontSize: 30, fontWeight: "600", letterSpacing: -0.6, color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(last())}</Text>
            <Text style={{ fontSize: 13, fontWeight: "500", color: toneColor(tone()), fontVariant: ["tabular-nums"] }}>
              {`${signed(change())}  ${signed((change() / OPEN) * 100)}%`}
            </Text>
          </View>
        </View>
        <Button
          label={running() ? "Pause" : "Resume"}
          variant="secondary"
          size="small"
          onPress={() => running.set(!running())}
        />
      </View>
      <View style={{ marginTop: size.gap }}>
        <Plot
          data={points()}
          y="price"
          yScale={{ type: "linear", domain: domain() }}
          appearGeneration={1}
          motion={{ appear: { duration: 900, easing: "easeOut" }, update: REFIT, enter: { duration: 120 }, exit: { duration: 160 }, pointer: { stiffness: 380, damping: 36 } }}
          accessibilityLabel={`NSHL, the last minute, ${fixed(last())}, ${signed((change() / OPEN) * 100)}% on the day`}
          accessibilityValue={{ text: hover() == null ? undefined : `${clock(points()[hover()!].t)}, ${fixed(points()[hover()!].price)}` }}
          crossLineClass="w-px h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.22) }}
          dotClass="size-2.5 rounded-full border-2"
          tooltipPlacement="pointer"
          tooltipOffset={10}
          tooltipContent={
            <View style={tipCard}>
              <Text style={tipTitle}>{clock(points()[tip()].t)}</Text>
              <Text style={{ fontSize: 13, fontWeight: "500", color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(points()[tip()].price)}</Text>
            </View>
          }
          onHoverChange={({ index }) => {
            hover.set(index);
            if (index != null) tip.set(index);
          }}
          focusStyle={focusRing(4)}
          style={{ height: LIVE_HEIGHT, marginRight: GUTTER, marginBottom: BELOW }}
        >
          <Grid y={grid()} style={{ stroke: colors.line }} />
          {/* The open: a faint dashed reference, while it is in view. */}
          <Grid y={OPEN > domain()[0] && OPEN < domain()[1] ? [OPEN] : []} style={{ stroke: alpha(colors.text, 0.28), strokeDasharray: "2 4" }} />
          <Area backgroundStyle={wash(toneColor(tone()), 0.2)} hoverDotClass="hidden" />
          {/* The channel: each tick's high and low of the ten before it, the line running inside. */}
          <Area y1="high" y0="low" style={{ fill: alpha(toneColor(tone()), 0.12) }} hoverDotClass="hidden" />
          <Line style={{ stroke: toneColor(tone()), strokeWidth: 1.75 }} hoverDotStyle={{ backgroundColor: toneColor(tone()), borderColor: colors.inset }} />
          {/* The newest point: a dot ringed in the inset's color, its halo breathing. */}
          <View style={{ position: "absolute", left: "100%", top: y(last()), width: 0, height: 0, pointerEvents: "none" }}>
            <If when={breathing()}>
              <View class="animate-ping" style={{ position: "absolute", left: -6, top: -6, width: 12, height: 12, borderRadius: 6, backgroundColor: alpha(toneColor(tone()), 0.5) }} />
            </If>
            <View style={{ position: "absolute", left: -4.5, top: -4.5, width: 9, height: 9, borderRadius: 4.5, borderWidth: 2, borderColor: colors.inset, backgroundColor: toneColor(tone()) }} />
          </View>
          {/* The gutter: the grid's values clear of the last price's tag. */}
          <PlotAxis
            placement="right"
            tickValues={grid().filter((v) => Math.abs(y(v) - y(last())) > 14)}
            format={(v) => fixed(v)}
            labelStyle={{ ...axis, paddingLeft: 10 }}
          />
          {/* The window's high over its point, and the last price's tag at its height in the gutter. */}
          <PlotLabel
            texts={[
              {
                text: `High ${fixed(high())}`,
                x: peak(),
                y: high(),
                align: peakAlign(),
                class: "-translate-y-full",
                // On the card's color, so a grid line near the high passes behind it.
                style: { ...axis, color: colors.muted, marginTop: -6, paddingHorizontal: 4, borderRadius: 3, backgroundColor: colors.surface },
              },
              {
                text: fixed(last()),
                x: LIVE_POINTS - 1,
                y: last(),
                class: "-translate-y-1/2",
                style: {
                  marginLeft: 6,
                  paddingHorizontal: 6,
                  lineHeight: 20,
                  borderRadius: 6,
                  backgroundColor: toneColor(tone()),
                  fontSize: 11,
                  fontFamily: mono,
                  fontWeight: "600",
                  color: colors.surface,
                  fontVariant: ["tabular-nums"],
                },
              },
            ]}
          />
          {/* The window's seconds under it, the first and last at its ends. */}
          <PlotAxis ticks={SECONDS.length} format={(_, i) => SECONDS[i]} labelStyle={below} />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="Open" value={fixed(OPEN)} />
          <Figure label="High" value={fixed(high())} />
          <Figure label="Low" value={fixed(low())} />
          <Figure label="Range" value={`${fixed(((high() - low()) / low()) * 100)}%`} />
        </Figures>
      </View>
    </Card>
  );
}

/** The book around the middle price: levels a tick apart on each side. */
const MID = 100;
const TICK = 0.02;
const LEVELS = 40;
type Level = { price: number; bid?: number; ask?: number };
/** Each side's sizes, thicker away from the middle, with a few walls. */
const SIDES = (() => {
  const r = random(83);
  const side = () =>
    Array.from({ length: LEVELS }, (_, k) => {
      const wall = r() > 0.9 ? 3.2 : 1;
      return Math.round((30 + r() * 140) * (1 + k / 14) * wall);
    });
  return { bids: side(), asks: side() };
})();
/** The levels within `n` ticks of the best prices, from the lowest bid up: each
 * side's size summed from the best price out. */
function book(n: number): Level[] {
  const bids: Level[] = [];
  let sum = 0;
  for (let k = 0; k < n; k++) {
    sum += SIDES.bids[k];
    bids.unshift({ price: Math.round((MID - TICK - k * TICK) * 100) / 100, bid: sum });
  }
  const asks: Level[] = [];
  sum = 0;
  for (let k = 0; k < n; k++) {
    sum += SIDES.asks[k];
    asks.push({ price: Math.round((MID + TICK + k * TICK) * 100) / 100, ask: sum });
  }
  return [...bids, ...asks];
}
const DEPTH_HEIGHT = 168;
const RANGES = [
  { value: "10", label: "10 ticks" },
  { value: "20", label: "20 ticks" },
  { value: "40", label: "40 ticks" },
] as const;

/**
 * An order book's depth: the bids' size summed from the best bid down on
 * the left, the asks' from the best ask up on the right, as steps (a
 * level's size holds until the next one), the spread a gap in the middle.
 * The side under the pointer shows its dot and its sum; the bar under the
 * plot weighs one side against the other.
 */
function Depth() {
  const range = signal<(typeof RANGES)[number]["value"]>("20");
  const rows = () => book(Number(range()));
  const hover = signal<number | null>(null);
  const tip = signal(0);
  const lo = () => rows()[0].price - TICK;
  const hi = () => rows()[rows().length - 1].price + TICK;
  const top = () => Math.max(...rows().map((r) => r.bid ?? r.ask ?? 0)) * 1.18;
  const at = (price: number): `${number}%` => `${((price - lo()) / (hi() - lo())) * 100}%`;
  const bids = () => rows().reduce((s, r) => Math.max(s, r.bid ?? 0), 0);
  const asks = () => rows().reduce((s, r) => Math.max(s, r.ask ?? 0), 0);
  const shown = () => rows()[tip()] ?? rows()[0];
  const side = () => (shown().bid != null ? "bid" : "ask");
  const hoveredSide = () => (hover() == null ? null : side());
  // A side's dot shows only while the pointer is over that side.
  const dot = (which: "bid" | "ask", tone: Tone) => ({
    backgroundColor: toneColor(tone),
    borderColor: colors.inset,
    opacity: hoveredSide() === which ? 1 : 0,
  });
  return (
    <Card icon="stocks" title="Order book depth" subtitle="Each side summed out from the spread, as steps">
      <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
        <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(MID)}</Text>
        <Text style={{ fontSize: 12, color: colors.muted, fontVariant: ["tabular-nums"] }}>{`mid · spread ${fixed(TICK * 2)} (${fixed(((TICK * 2) / MID) * 100)}%)`}</Text>
      </View>
      <View style={{ marginTop: size.gap }}>
        <Plot
          data={rows()}
          x="price"
          xScale={{ type: "linear", domain: [lo(), hi()] }}
          yScale={{ type: "linear", domain: [0, top()] }}
          appearGeneration={Number(range())}
          motion={{ appear: { duration: 700, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 160 }, pointer: { stiffness: 420, damping: 40 } }}
          accessibilityLabel={`Order book depth within ${range()} ticks: ${count(bids())} bid, ${count(asks())} asked`}
          accessibilityValue={{ text: hover() == null ? undefined : `${side() === "bid" ? "Bids" : "Asks"} to ${fixed(shown().price)}: ${count(shown().bid ?? shown().ask ?? 0)}` }}
          crossLineClass="w-px h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.22) }}
          dotClass="size-2.5 rounded-full border-2"
          tooltipOffset={6}
          tooltipContent={
            <View style={tipCard}>
              <Text style={{ ...tipTitle, color: toneColor(side() === "bid" ? "up" : "down") }}>{side() === "bid" ? `BIDS ≥ ${fixed(shown().price)}` : `ASKS ≤ ${fixed(shown().price)}`}</Text>
              <Text style={{ fontSize: 13, fontWeight: "500", color: colors.text, fontVariant: ["tabular-nums"] }}>{count(shown().bid ?? shown().ask ?? 0)}</Text>
            </View>
          }
          onHoverChange={({ index }) => {
            hover.set(index);
            if (index != null) tip.set(index);
          }}
          focusStyle={focusRing(4)}
          style={{ height: DEPTH_HEIGHT, marginBottom: BELOW }}
        >
          <Grid y={ticks(0, top(), 3)} style={{ stroke: colors.line }} />
          {/* The middle: a dashed hairline in the spread. */}
          <View style={{ position: "absolute", left: at(MID), top: 0, bottom: 0, borderLeftWidth: 1, borderStyle: "dashed", borderColor: alpha(colors.text, 0.2), pointerEvents: "none" }} />
          <Area y1="bid" curve="stepAfter" backgroundStyle={wash(toneColor("up"), 0.22)} hoverDotClass="hidden" />
          <Area y1="ask" curve="stepAfter" backgroundStyle={wash(toneColor("down"), 0.22)} hoverDotClass="hidden" />
          <Line y="bid" curve="stepAfter" style={{ stroke: toneColor("up"), strokeWidth: 1.5 }} hoverDotStyle={dot("bid", "up")} />
          <Line y="ask" curve="stepAfter" style={{ stroke: toneColor("down"), strokeWidth: 1.5 }} hoverDotStyle={dot("ask", "down")} />
          <Grid y={[0]} style={{ stroke: colors.border }} />
          {/* The prices at the ends, and the middle a shade firmer. */}
          <PlotAxis tickValues={[lo(), hi()]} format={(v) => fixed(v)} labelStyle={below} />
          <PlotAxis tickValues={[MID]} format={(v) => fixed(v)} labelStyle={{ ...below, color: colors.muted }} />
        </Plot>
      </View>
      {/* The balance: each side's share of what the book holds in view. */}
      <View style={{ marginTop: size.gap + 4, gap: 8 }}>
        <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
          <Text style={{ fontSize: 12, color: toneColor("up"), fontVariant: ["tabular-nums"] }}>{`Bids ${fixed((bids() / (bids() + asks())) * 100, 0)}%`}</Text>
          <Text style={{ fontSize: 12, color: toneColor("down"), fontVariant: ["tabular-nums"] }}>{`${fixed((asks() / (bids() + asks())) * 100, 0)}% Asks`}</Text>
        </View>
        <View style={{ flexDirection: "row", gap: 3, height: 4 }}>
          <View class="transition-all duration-500 motion-reduce:transition-none" style={{ flex: bids(), borderRadius: 2, backgroundColor: toneColor("up") }} />
          <View class="transition-all duration-500 motion-reduce:transition-none" style={{ flex: asks(), borderRadius: 2, backgroundColor: toneColor("down") }} />
        </View>
      </View>
      <View style={{ marginTop: size.gap + 2, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 260 : undefined }}>
        <Segmented options={RANGES} value={range()} size={size.switch} onChange={(next) => range.set(next)} />
      </View>
    </Card>
  );
}

/** A portfolio's value over 120 days and how far each day sits under its best so far. */
const EQUITY = (() => {
  const r = random(5);
  let value = 10000;
  let peak = value;
  return Array.from({ length: 120 }, (_, day) => {
    // A rally, a slide, a recovery: drift that changes sign with the phase.
    const drift = day < 38 ? 0.004 : day < 66 ? -0.0062 : 0.0058;
    value *= 1 + drift + (r() - 0.5) * 0.018;
    peak = Math.max(peak, value);
    return { day, value: Math.round(value), drawdown: (value / peak - 1) * 100 };
  });
})();
const TROUGH = EQUITY.reduce((a, b) => (b.drawdown < a.drawdown ? b : a));
const PEAK = EQUITY.slice(0, TROUGH.day + 1).reduce((a, b) => (b.value > a.value ? b : a));
/** The days of the worst run, from its high to its low. */
const WORST_RUN = EQUITY.slice(PEAK.day, TROUGH.day + 1);
const RECOVERED = EQUITY.find((d) => d.day > TROUGH.day && d.value >= PEAK.value);
const UNDERWATER = EQUITY.filter((d) => d.drawdown < -0.05).length / EQUITY.length;
const EQUITY_HEIGHT = 140;
const WATER_HEIGHT = 64;
const DD_FLOOR = Math.floor(TROUGH.drawdown / 5) * 5 - 2;
const VALUE_LO = Math.min(...EQUITY.map((d) => d.value)) * 0.97;
const VALUE_HI = Math.max(...EQUITY.map((d) => d.value)) * 1.03;
/** Where day `i` is across both plots (their rows are evenly spread). */
const across = (i: number): `${number}%` => `${(i / (EQUITY.length - 1)) * 100}%`;
/** The rows of the days the axis names: day 1, then days 30, 60, 90 and 120. */
const DAYS = [0, 29, 59, 89, 119];

/**
 * A portfolio's value over its drawdown: the equity curve, the worst run
 * from a high to its low shaded on it, and under it the "underwater" plot,
 * how far each day sits below the best day before it. The day under the
 * pointer is marked on both plots and read out above them.
 */
function Drawdown() {
  const hover = signal<number | null>(null);
  // Which plot the pointer is over: the other one marks the day itself.
  const over = signal<"equity" | "water" | null>(null);
  const shown = () => EQUITY[hover() ?? EQUITY.length - 1];
  const follow = (which: "equity" | "water") => ({ index }: { index: number | null }) => {
    hover.set(index);
    over.set(index == null ? null : which);
  };
  // Where the hovered day is on each plot, from its own scale.
  const height = (which: "equity" | "water") =>
    which === "equity"
      ? (1 - (shown().value - VALUE_LO) / (VALUE_HI - VALUE_LO)) * EQUITY_HEIGHT
      : (shown().drawdown / DD_FLOOR) * WATER_HEIGHT;
  const mark = (which: "equity" | "water", tone: Tone) => (
    <If when={hover() != null && over() !== which}>
      <View style={{ position: "absolute", left: across(hover() ?? 0), top: 0, bottom: 0, width: 1, backgroundColor: alpha(colors.text, 0.22), pointerEvents: "none" }} />
      <View style={{ position: "absolute", left: across(hover() ?? 0), top: height(which), width: 0, height: 0, pointerEvents: "none" }}>
        <View style={{ position: "absolute", left: -4, top: -4, width: 8, height: 8, borderRadius: 4, borderWidth: 2, borderColor: colors.inset, backgroundColor: toneColor(tone) }} />
      </View>
    </If>
  );
  const total = (EQUITY[EQUITY.length - 1].value / EQUITY[0].value - 1) * 100;
  return (
    <Card icon="stocks" title="Drawdown" subtitle="Value over its distance from the high, the worst run shaded">
      <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
        <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{`$${count(shown().value)}`}</Text>
        <Text style={{ fontSize: 12, color: shown().drawdown < -0.05 ? toneColor("danger") : colors.muted, fontVariant: ["tabular-nums"] }}>
          {shown().drawdown < -0.05 ? `${signed(shown().drawdown, 1)}% from the high` : "at a high"}
        </Text>
        <View style={{ flex: 1 }} />
        <Text style={{ ...axis, color: colors.muted }}>{`DAY ${shown().day + 1}`}</Text>
      </View>
      <View style={{ marginTop: size.gap, gap: 6 }}>
        <Plot
          data={EQUITY}
          // Days on a linear scale, so the worst run's own rows land on their days.
          x="day"
          xScale={{ type: "linear", domain: [0, EQUITY.length - 1] }}
          y="value"
          yScale={{ type: "linear", domain: [VALUE_LO, VALUE_HI] }}
          appearGeneration={1}
          // No pointer spring: the other plot marks the same day at once, so both move together.
          motion={{ appear: { duration: 1000, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 160 } }}
          accessibilityLabel={`Portfolio value over 120 days, ${signed(total, 1)}%; worst drawdown ${signed(TROUGH.drawdown, 1)}% from day ${PEAK.day + 1} to day ${TROUGH.day + 1}`}
          accessibilityValue={{ text: hover() == null ? undefined : `Day ${shown().day + 1}, $${count(shown().value)}, ${signed(shown().drawdown, 1)}%` }}
          crossLineClass="w-px h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.22) }}
          dotClass="size-2.5 rounded-full border-2"
          onHoverChange={follow("equity")}
          focusStyle={focusRing(4)}
          style={{ height: EQUITY_HEIGHT }}
        >
          {/* The worst run, from its high to its low, its value hatched under the curve. */}
          <View style={{ position: "absolute", left: across(PEAK.day), width: `${((TROUGH.day - PEAK.day) / (EQUITY.length - 1)) * 100}%`, top: 0, bottom: 0, backgroundColor: alpha(colors.danger, 0.04), pointerEvents: "none" }}>
            <Text style={{ ...axis, position: "absolute", left: 6, top: 6, color: toneColor("danger") }}>{`MAX ${signed(TROUGH.drawdown, 1)}%`}</Text>
          </View>
          <Grid y={ticks(VALUE_LO, VALUE_HI, 3)} style={{ stroke: colors.line }} />
          <Area backgroundStyle={wash(toneColor("accent"), 0.16)} hoverDotClass="hidden" />
          {/* Hairline stripes 6pt apart, falling to the right. */}
          <Area
            data={WORST_RUN}
            x="day"
            backgroundStyle={{ backgroundImage: `repeating-linear-gradient(-45deg, ${alpha(colors.danger, 0.32)} 0px, ${alpha(colors.danger, 0.32)} 1px, transparent 1px, transparent 6px)` }}
            hoverDotClass="hidden"
          />
          <Line style={{ stroke: toneColor("accent"), strokeWidth: 1.75 }} hoverDotStyle={{ backgroundColor: toneColor("accent"), borderColor: colors.inset }} />
          {mark("equity", "accent")}
        </Plot>
        {/* Underwater: hanging from zero, deeper red the further down. */}
        <Plot
          data={EQUITY}
          y="drawdown"
          yScale={{ type: "linear", domain: [DD_FLOOR, 0] }}
          appearGeneration={1}
          motion={{ appear: { duration: 1000, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 160 } }}
          accessibilityLabel={`Drawdown from the high, at worst ${signed(TROUGH.drawdown, 1)}% on day ${TROUGH.day + 1}`}
          accessibilityValue={{ text: hover() == null ? undefined : `Day ${shown().day + 1}, ${signed(shown().drawdown, 1)}% from the high` }}
          crossLineClass="w-px h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.22) }}
          dotClass="size-2 rounded-full border-2"
          onHoverChange={follow("water")}
          focusStyle={focusRing(4)}
          style={{ height: WATER_HEIGHT, marginBottom: BELOW }}
        >
          <Area
            y0={0}
            backgroundStyle={{ backgroundImage: `linear-gradient(to bottom, ${alpha(colors.danger, 0.04)}, ${alpha(colors.danger, 0.26)})` }}
            hoverDotClass="hidden"
          />
          <Line style={{ stroke: toneColor("danger"), strokeWidth: 1.25 }} hoverDotStyle={{ backgroundColor: toneColor("danger"), borderColor: colors.inset }} />
          {/* The floor's value over its edge, and the low marked where it is. */}
          <PlotAxis placement="left" labelPlacement="inside" tickValues={[DD_FLOOR]} format={(v) => `${v}%`} labelStyle={{ ...axis, paddingBottom: 2 }} />
          <View style={{ position: "absolute", left: across(TROUGH.day), top: (TROUGH.drawdown / DD_FLOOR) * WATER_HEIGHT, width: 0, height: 0, pointerEvents: "none" }}>
            <View style={{ position: "absolute", left: -3.5, top: -3.5, width: 7, height: 7, borderRadius: 3.5, borderWidth: 1.5, borderColor: colors.inset, backgroundColor: toneColor("danger") }} />
            {/* A width of its own: the zero-width anchor would wrap it at every break. */}
            <Text style={{ ...axis, position: "absolute", left: 8, top: -6, width: 64, color: toneColor("danger") }}>{`${signed(TROUGH.drawdown, 1)}%`}</Text>
          </View>
          {mark("water", "danger")}
          {/* The days under both plots: the first, then every 30th. */}
          <PlotAxis tickValues={DAYS} format={(v) => (v === 0 ? "Day 1" : String(v + 1))} labelStyle={below} />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="Return" value={`${signed(total, 1)}%`} tone={total >= 0 ? "up" : "down"} />
          <Figure label="Worst" value={`${signed(TROUGH.drawdown, 1)}%`} tone="danger" />
          <Figure label="Recovery" value={RECOVERED ? `${RECOVERED.day - TROUGH.day} days` : "—"} />
          <Figure label="Underwater" value={`${fixed(UNDERWATER * 100, 0)}%`} />
        </Figures>
      </View>
    </Card>
  );
}

/** Three resources' load, by period. */
const PERIODS = [
  { value: "now", label: "Now" },
  { value: "day", label: "24h peak" },
  { value: "week", label: "7d peak" },
] as const;
type Period = (typeof PERIODS)[number]["value"];
const RESOURCES: { name: string; color: string; unit: string; total: number; used: Record<Period, number> }[] = [
  { name: "Compute", color: chart.s1, unit: "vCPU", total: 64, used: { now: 38, day: 51, week: 59 } },
  { name: "Memory", color: chart.s2, unit: "GB", total: 256, used: { now: 141, day: 188, week: 214 } },
  { name: "Network", color: chart.s3, unit: "Gbps", total: 10, used: { now: 3.2, day: 7.4, week: 8.9 } },
];
const RINGS = pointer ? 164 : 172;
/** Each ring's width and the gap between rings, in points. */
const RING_WIDTH = 11;
const RING_GAP = 5;

/**
 * Capacity as rings: a track per resource and over it the share in use,
 * with round caps, sweeping in from 12 o'clock and then round to each
 * period's load (`motion.update`); the average in the middle and the
 * figures beside, in the rings' colors.
 */
function Capacity() {
  const period = signal<Period>("now");
  const share = (r: (typeof RESOURCES)[number]) => r.used[period()] / r.total;
  const average = () => RESOURCES.reduce((s, r) => s + share(r), 0) / RESOURCES.length;
  const outer = RINGS / 2 - 2;
  const radii = RESOURCES.map((_, i) => {
    const o = outer - i * (RING_WIDTH + RING_GAP);
    return { outer: o, inner: o - RING_WIDTH };
  });
  return (
    <Card icon="system" title="Capacity" subtitle="Rings of load against each resource's limit">
      <View style={{ flexDirection: pointer ? "row" : "column", alignItems: "center", gap: pointer ? 24 : 16 }}>
        <Plot
          data={RESOURCES}
          interactive={false}
          appearGeneration={1}
          motion={{ appear: { duration: 1100, easing: "easeOut" }, update: { duration: 700, easing: "easeInOut" } }}
          accessibilityLabel={`Capacity, ${PERIODS.find((p) => p.value === period())!.label}: ${RESOURCES.map((r) => `${r.name} ${fixed(share(r) * 100, 0)}%`).join(", ")}`}
          style={{ width: RINGS, height: RINGS }}
        >
          {RESOURCES.map((r, i) => (
            <Arc innerRadius={radii[i].inner} outerRadius={radii[i].outer} style={{ fill: alpha(r.color, 0.13) }} />
          ))}
          {RESOURCES.map((r, i) => (
            <Arc
              innerRadius={radii[i].inner}
              outerRadius={radii[i].outer}
              endAngle={share(r) * 2 * Math.PI}
              cornerRadius={RING_WIDTH / 2}
              style={{ fill: r.color }}
            />
          ))}
          <View style={{ position: "absolute", left: 0, top: 0, right: 0, bottom: 0, alignItems: "center", justifyContent: "center", gap: 2, pointerEvents: "none" }}>
            <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{`${fixed(average() * 100, 0)}%`}</Text>
            <Text style={{ fontSize: 10, fontFamily: mono, letterSpacing: 0.8, color: colors.faint }}>AVG LOAD</Text>
          </View>
        </Plot>
        <View style={{ alignSelf: "stretch", flex: pointer ? 1 : undefined, justifyContent: "center", gap: 14 }}>
          {RESOURCES.map((r) => (
            <View style={{ gap: 6 }}>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: r.color }} />
                <Text style={{ flex: 1, fontSize: 13, color: colors.text }}>{r.name}</Text>
                <Text style={{ fontSize: 15, fontWeight: "600", color: colors.text, fontVariant: ["tabular-nums"] }}>{`${fixed(share(r) * 100, 0)}%`}</Text>
              </View>
              <Text style={{ ...axis, marginLeft: 16 }}>{`${r.used[period()]} / ${r.total} ${r.unit}`}</Text>
            </View>
          ))}
        </View>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 260 : undefined }}>
        <Segmented options={PERIODS} value={period()} size={size.switch} onChange={(next) => period.set(next)} />
      </View>
    </Card>
  );
}

/** Revenue of a quarter by region, in thousands, the largest first. */
const REGIONS = [
  { name: "North America", revenue: 482 },
  { name: "EMEA", revenue: 312 },
  { name: "APAC", revenue: 268 },
  { name: "Latin America", revenue: 104 },
  { name: "Other", revenue: 74 },
];
const REVENUE = REGIONS.reduce((sum, r) => sum + r.revenue, 0);
/** The theme's series colors, in the regions' order (read where used, so
 * they follow the appearance): the plot's ordinal `colorScale` gives each
 * region's slice the next one, as the legend gives its key. */
const palette = () => [chart.s1, chart.s2, chart.s3, chart.s4, chart.s5];
/** The leader lines of the pie's labels, each in its slice's color. */
const STROKES = ["stroke-chart-1", "stroke-chart-2", "stroke-chart-3", "stroke-chart-4", "stroke-chart-5"];
const money = (k: number) => (k >= 1000 ? `$${(k / 1000).toFixed(2)}M` : `$${k}k`);
const portion = (k: number) => `${((k / REVENUE) * 100).toFixed(1)}%`;
/** A slice's label: its share, whole. */
const percent = (k: number) => `${Math.round((k / REVENUE) * 100)}%`;
/** The ring's size, its rim 8 points in (room for the lift), the gaps
 * between slices 2 points wide there. */
const RING = pointer ? 168 : 176;
const RIM = RING / 2 - 8;
/** As a pie the ring draws in to leave room for its labels outside. */
const PIE_RIM = Math.round(RIM * 0.66);
/** The ring turned so its two smallest slices, the last, meet at 3
 * o'clock: their labels stack there instead of crowding side by side at
 * the top. */
const SMALLEST = REGIONS[3].revenue + REGIONS[4].revenue;
const PIE_START = Math.PI / 2 - (2 * Math.PI * (REVENUE - SMALLEST / 2)) / REVENUE;

/**
 * Revenue by region as a donut: 2-point gaps of even width, softly rounded
 * corners, the hovered slice lifted and the others dimmed, its value in
 * the hole, a legend with tabular figures at the trailing edge whose
 * hovered row stands out; it closes into a pie, the hole's radius moving
 * on `motion.update` as the ring draws in, its shares written outside
 * at the ends of leader lines in the slices' colors.
 */
function Revenue() {
  const hover = signal<number | null>(null);
  const donut = signal(true);
  const shown = () => (hover() == null ? undefined : REGIONS[hover()!]);
  return (
    <Card icon="stocks" title="Revenue by region" subtitle="A donut of shares, or a pie labeled around its rim">
      {/* The legend sits beside the ring where the card has room for its
          names and below it where it has not, never cutting a name short. */}
      <View style={{ flexDirection: pointer ? "row" : "column", flexWrap: "wrap", justifyContent: "center", alignItems: "center", gap: pointer ? 24 : 16 }}>
        <Plot
          data={REGIONS}
          accessibilityLabel={`Revenue by region, ${money(REVENUE)} in all; largest North America at ${portion(482)}`}
          accessibilityValue={{ text: shown() && `${shown()!.name}: ${money(shown()!.revenue)}, ${portion(shown()!.revenue)}` }}
          appearGeneration={1}
          motion={{ appear: { duration: 900, easing: "easeOut" }, update: { duration: 450, easing: "easeInOut" }, enter: { duration: 120 }, exit: { duration: 180 }, pointer: { stiffness: 420, damping: 40 } }}
          onHoverChange={({ index }) => hover.set(index)}
          focusStyle={{ ...focusRing(4), borderRadius: RING / 2 }}
          style={{ width: RING, height: RING }}
          colorScale={{ type: "ordinal", range: palette() }}
        >
          <Pie
            value="revenue"
            startAngle={PIE_START}
            endAngle={PIE_START + 2 * Math.PI}
            innerRadius={donut() ? RIM * 0.62 : 0}
            outerRadius={donut() ? RIM : PIE_RIM}
            padAngle={2 / RIM}
            cornerRadius={3}
            color="name"
            hoverLift={6}
            hoverDim={0.64}
            label={donut() ? undefined : (d: (typeof REGIONS)[number]) => percent(d.revenue)}
            labelGap={10}
            labelStyle={{ paddingHorizontal: 4, fontSize: 11, fontFamily: mono, color: colors.muted, fontVariant: ["tabular-nums"] }}
            labelLineClass={(_, i) => STROKES[i % STROKES.length]}
            labelLineStyle={{ strokeWidth: 1 }}
          />
          {/* The total in the hole fades as it opens and closes. */}
          <View
            class="transition-opacity duration-300 motion-reduce:transition-none"
            style={{ pointerEvents: "none", position: "absolute", left: 0, top: 0, right: 0, bottom: 0, alignItems: "center", justifyContent: "center", gap: 2, opacity: donut() ? 1 : 0 }}
          >
            <Text style={{ fontSize: 10, fontFamily: mono, letterSpacing: 0.8, color: colors.faint }}>{(shown()?.name ?? "Total").toUpperCase()}</Text>
            <Text style={{ fontSize: 20, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{money(shown()?.revenue ?? REVENUE)}</Text>
          </View>
        </Plot>
        <View style={{ alignSelf: "stretch", flexGrow: 1, flexBasis: pointer ? 260 : undefined, justifyContent: "center", gap: 2 }}>
          {REGIONS.map((r, i) => (
            <View
              class="transition-opacity duration-150 motion-reduce:transition-none"
              style={{
                flexDirection: "row",
                alignItems: "center",
                gap: 10,
                height: pointer ? 30 : 36,
                paddingHorizontal: 8,
                borderRadius: 6,
                backgroundColor: hover() === i ? alpha(colors.text, 0.05) : undefined,
                opacity: hover() == null || hover() === i ? 1 : 0.45,
              }}
            >
              <View style={{ width: 8, height: 8, borderRadius: 2, backgroundColor: palette()[i] }} />
              <Text style={{ numberOfLines: 1, flex: 1, fontSize: 13, color: colors.text }}>{r.name}</Text>
              <Text style={{ fontSize: 13, color: colors.text, fontVariant: ["tabular-nums"], textAlign: "right" }}>{money(r.revenue)}</Text>
              <Text style={{ width: 48, fontSize: 13, color: colors.muted, fontVariant: ["tabular-nums"], textAlign: "right" }}>{portion(r.revenue)}</Text>
            </View>
          ))}
        </View>
      </View>
      <View style={{ flexDirection: "row", marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch" }}>
        <Button
          label={donut() ? "Show as pie" : "Show as donut"}
          variant="secondary"
          size={size.switch}
          flex={pointer ? undefined : 1}
          onPress={() => {
            donut.set(!donut());
            hover.set(null);
          }}
        />
      </View>
    </Card>
  );
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul"];
/** Sales per month, in thousands, online and in store. */
const SALES = [
  [42, 31],
  [51, 28],
  [48, 36],
  [63, 33],
  [58, 40],
  [71, 38],
  [66, 44],
].map(([online, store], i) => ({ month: MONTHS[i], online, store }));
type Sale = (typeof SALES)[number];
/** The top of the y scale: a step above the highest grid line, room for
 * the labels over the stacks. */
const SALES_TOP = 130;
const SALES_HEIGHT = 168;
/** The width of the y axis' labels. */
const AXIS = 28;

/**
 * Sales per month as bars in bands, side by side or stacked, growing in
 * one after another, a value over each (the total over a stack), the month
 * under the pointer a faint band over its bars while the others fade.
 */
function Sales() {
  const mode = signal<"grouped" | "stacked">("grouped");
  const hover = signal<number | null>(null);
  const stacked = () => mode() === "stacked";
  const shown = () => SALES[hover() ?? SALES.length - 1];
  const total = (d: Sale) => d.online + d.store;
  // Upright bars, their tips rounded; a stack only at its top.
  const tip = { topLeft: 3, topRight: 3 };
  const labelStyle = { fontSize: 10.5, fontFamily: mono, paddingBottom: 3, color: colors.muted, fontVariant: ["tabular-nums" as const] };
  return (
    <Card icon="stocks" title="Sales" subtitle="Bars per month, side by side or stacked">
      <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
        <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{`${total(shown())}k`}</Text>
        <Text style={{ fontSize: 12, color: colors.muted }}>{hover() == null ? `${shown().month}, the last month` : shown().month}</Text>
      </View>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 16, marginTop: 8 }}>
        <Legend color={chart.s1} label="Online" value={`${shown().online}k`} />
        <Legend color={chart.s2} label="In store" value={`${shown().store}k`} />
      </View>
      <View style={{ marginTop: size.gap }}>
        <Plot
          data={SALES}
          // Bands with half their gap at the ends: their middles divide
          // the width evenly, each month's label centered under its band.
          xScale={{ type: "band", paddingInner: 0.3, paddingOuter: 0.15, maxBandWidth: 56, groupPadding: 0.12 }}
          yScale={{ type: "linear", domain: [0, SALES_TOP] }}
          appearGeneration={stacked() ? 1 : 0}
          motion={{ appear: { duration: 600, easing: "easeOut" }, enter: { duration: 150 }, exit: { duration: 150 }, pointer: { stiffness: 320, damping: 32 } }}
          accessibilityLabel={`Sales per month, online and in store, highest in ${SALES.reduce((a, b) => (total(b) > total(a) ? b : a)).month}`}
          accessibilityValue={{ text: hover() == null ? undefined : `${shown().month}, ${shown().online}k online, ${shown().store}k in store` }}
          // The band of the month under the pointer: a faint tint over it.
          crossLineClass="w-full h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.05) }}
          onHoverChange={({ index }) => hover.set(index)}
          focusStyle={focusRing(4)}
          style={{ height: SALES_HEIGHT, marginRight: AXIS + 8, marginBottom: BELOW }}
        >
          <Grid y={[40, 80, 120]} style={{ stroke: colors.line }} />
          <Grid y={[0]} style={{ stroke: colors.border }} />
          <Bar
            value="online"
            stack={stacked() ? "sales" : undefined}
            cornerRadii={stacked() ? 0 : tip}
            stagger={0.4}
            inactiveOpacity={0.45}
            label={stacked() ? undefined : "online"}
            labelStyle={labelStyle}
            style={{ fill: chart.s1 }}
          />
          <Bar
            value="store"
            stack={stacked() ? "sales" : undefined}
            cornerRadii={tip}
            stagger={0.4}
            inactiveOpacity={0.45}
            // Stacked, the top bar's label is the month's total.
            label={stacked() ? (d: Sale) => String(total(d)) : "store"}
            labelStyle={labelStyle}
            style={{ fill: chart.s2 }}
          />
          {/* The grid's values right of it, as d3 writes them, and a month under each band. */}
          <PlotAxis placement="right" tickValues={[0, 40, 80, 120]} labelStyle={{ ...axis, width: AXIS + 8, paddingLeft: 8, textAlign: "right" }} />
          <PlotAxis format={(v) => MONTHS[v]} labelStyle={below} />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 220 : undefined }}>
        <Segmented
          options={[
            { value: "grouped", label: "Side by side" },
            { value: "stacked", label: "Stacked" },
          ]}
          value={mode()}
          size={size.switch}
          onChange={(next) => mode.set(next)}
        />
      </View>
    </Card>
  );
}

/** Where band `i` of `n` starts and how wide it is, and the step between
 * two bands' starts, as shares of the plot's width: the band scale's own
 * arithmetic (no `maxBandWidth`), so a view of the app's lines up with a
 * band. */
function bandAt(i: number, n: number, pad: { inner: number; outer: number }) {
  const step = 1 / (n - pad.inner + 2 * pad.outer);
  return { start: step * (pad.outer + i), width: step * (1 - pad.inner), step };
}
const share = (v: number): `${number}%` => `${v * 100}%`;

/** The bridge's quarters. */
const BRIDGE_QUARTERS = [
  { value: "1Q26", label: "1Q26" },
  { value: "2Q26", label: "2Q26" },
  { value: "3Q26", label: "3Q26" },
  { value: "4Q26", label: "4Q26" },
] as const;
type BridgeQuarter = (typeof BRIDGE_QUARTERS)[number]["value"];
/** Each quarter's revenue and what each line after it adds or takes, in
 * millions: cost of sales, R&D, SG&A, other income (a loss in 3Q), tax. */
const STATEMENTS: Record<BridgeQuarter, number[]> = {
  "1Q26": [118.4, -46.2, -19.8, -24.1, 2.6, -7.4],
  "2Q26": [126.9, -48.5, -20.6, -25.3, 1.8, -8.1],
  "3Q26": [131.2, -52.9, -21.9, -26.0, -1.4, -6.6],
  "4Q26": [142.7, -54.1, -22.8, -27.4, 3.9, -9.2],
};
type StepKind = "total" | "increase" | "decrease";
type Step = { name: string; short: string; kind: StepKind; start: number; end: number };
/** A quarter's bridge: totals stand on zero, every other step runs from
 * where the one before it ends to where it leaves the running total. */
function bridge(q: BridgeQuarter): Step[] {
  const [revenue, cost, rd, sga, other, tax] = STATEMENTS[q];
  const steps: Step[] = [];
  let level = 0;
  const total = (name: string, short: string) => steps.push({ name, short, kind: "total", start: 0, end: level });
  const line = (name: string, short: string, v: number) => {
    const end = Math.round((level + v) * 10) / 10;
    steps.push({ name, short, kind: v >= 0 ? "increase" : "decrease", start: level, end });
    level = end;
  };
  level = revenue;
  total("Revenue", "Rev");
  line("Cost of sales", "COGS", cost);
  total("Gross profit", "Gross");
  line("R&D", "R&D", rd);
  line("SG&A", "SG&A", sga);
  line(other >= 0 ? "Other income" : "Other loss", "Other", other);
  line("Tax", "Tax", tax);
  total("Net profit", "Net");
  return steps;
}
const BRIDGE_PAD = { inner: 0.32, outer: 0.16 };
const BRIDGE_TOP = 160;
const BRIDGE_HEIGHT = 176;
/** How a quarter's steps move into the next one's: the bars and their
 * labels on the plot's `motion.update`, the connectors with a layout
 * animation of the same length. */
const BRIDGE_UPDATE = { duration: 550, easing: "easeInOut" } as const;
const bridgeLayout = {
  duration: BRIDGE_UPDATE.duration,
  update: { type: "easeInEaseOut" },
  create: { type: "easeInEaseOut", property: "opacity" },
  delete: { type: "easeInEaseOut", property: "opacity" },
} as const;
/** The gaps between the bridge's eight steps, each crossed by a connector. */
const CONNECTORS = [0, 1, 2, 3, 4, 5, 6];
const millions = (v: number) => `$${fixed(v, 1)}M`;
const signedMillions = (v: number) => `${v >= 0 ? "+" : "−"}$${fixed(Math.abs(v), 1)}M`;
/** What a step shows: a total's level, any other step's change. */
const amount = (s: Step) => (s.kind === "total" ? s.end : s.end - s.start);

/**
 * A quarter's profit bridge, as a waterfall: revenue, what each line of
 * the income statement takes or adds, gross and net profit as totals.
 * Each step is a `Bar` from its row's `base` (where the running total
 * stood) to its value (where the step leaves it), colored by kind through
 * the plot's `colorScale`, its signed change written at its end (above a
 * rise, below a fall); dashed connectors carry each level to the next
 * step. The step under the pointer or the arrow keys is read out over the
 * plot and its share of revenue; the quarters move into each other.
 */
function Bridge() {
  const quarter = signal<BridgeQuarter>("4Q26");
  const hover = signal<number | null>(null);
  const steps = () => bridge(quarter());
  const revenue = () => steps()[0].end;
  const net = () => steps()[steps().length - 1].end;
  const operating = () => steps()[4].end;
  const margin = (v: number) => `${fixed((v / revenue()) * 100, 1)}%`;
  const shown = () => (hover() == null ? undefined : steps()[hover()!]);
  const toneOf = (s: Step): Tone => (s.kind === "increase" ? "up" : s.kind === "decrease" ? "down" : "neutral");
  const labelStyle = { fontSize: 10.5, fontFamily: mono, paddingVertical: 3, color: colors.muted, fontVariant: ["tabular-nums" as const] };
  return (
    <Card icon="stocks" title="Profit bridge" subtitle="A quarter's revenue down to net profit, step by step">
      <View style={{ gap: 4 }}>
        <Text style={tipTitle}>{(shown()?.name ?? "Net profit").toUpperCase()}</Text>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: shown() ? toneColor(toneOf(shown()!)) : colors.text, fontVariant: ["tabular-nums"] }}>
            {shown() ? (shown()!.kind === "total" ? millions(shown()!.end) : signedMillions(amount(shown()!))) : millions(net())}
          </Text>
          <Text style={{ fontSize: 12, color: colors.muted, fontVariant: ["tabular-nums"] }}>
            {shown() ? `${margin(Math.abs(amount(shown()!)))} of revenue` : `${margin(net())} net margin`}
          </Text>
        </View>
      </View>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 16, marginTop: 8 }}>
        <Legend color={chart.s2} label="Total" />
        <Legend color={colors.up} label="Adds" />
        <Legend color={colors.down} label="Takes" />
      </View>
      <View style={{ marginTop: size.gap }}>
        <Plot
          data={steps()}
          xScale={{ type: "band", paddingInner: BRIDGE_PAD.inner, paddingOuter: BRIDGE_PAD.outer }}
          yScale={{ type: "linear", domain: [0, BRIDGE_TOP] }}
          // Each step's kind takes its color: totals graphite, rises and falls in the day's tones.
          colorScale={{ type: "ordinal", domain: ["total", "increase", "decrease"], range: [chart.s2, colors.up, colors.down] }}
          appearGeneration={1}
          motion={{ appear: { duration: 900, easing: "easeOut" }, update: BRIDGE_UPDATE, enter: { duration: 150 }, exit: { duration: 150 }, pointer: { stiffness: 320, damping: 32 } }}
          accessibilityLabel={`Profit bridge, ${quarter()}: ${millions(revenue())} revenue to ${millions(net())} net profit, a net margin of ${margin(net())}`}
          accessibilityValue={{ text: shown() && `${shown()!.name}: ${shown()!.kind === "total" ? millions(shown()!.end) : signedMillions(amount(shown()!))}` }}
          crossLineClass="w-full h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.05) }}
          onHoverChange={({ index }) => hover.set(index)}
          focusStyle={focusRing(4)}
          style={{ height: BRIDGE_HEIGHT, marginRight: AXIS + 8, marginBottom: BELOW }}
        >
          <Grid y={[40, 80, 120]} style={{ stroke: colors.line }} />
          {/* Connectors: each step's level carried across the gap to the next step. */}
          {CONNECTORS.map((i) => {
            const band = bandAt(i, CONNECTORS.length + 1, BRIDGE_PAD);
            return (
              <View
                style={{
                  position: "absolute",
                  left: share(band.start + band.width),
                  width: share(band.step - band.width),
                  // Read here, so a quarter moves the same view.
                  top: (1 - steps()[i].end / BRIDGE_TOP) * BRIDGE_HEIGHT,
                  height: 0,
                  borderTopWidth: 1,
                  borderStyle: "dashed",
                  borderColor: alpha(colors.text, 0.3),
                  pointerEvents: "none",
                }}
              />
            );
          })}
          <Grid y={[0]} style={{ stroke: colors.border }} />
          <Bar
            value="end"
            base="start"
            color="kind"
            cornerRadii={2}
            stagger={0.5}
            inactiveOpacity={0.4}
            label={(d: Step) => (d.kind === "total" ? fixed(d.end, 1) : signed(d.end - d.start, 1))}
            labelStyle={labelStyle}
          />
          <PlotAxis placement="right" tickValues={[0, 40, 80, 120]} labelStyle={{ ...axis, width: AXIS + 8, paddingLeft: 8, textAlign: "right" }} />
          <PlotAxis format={(v) => steps()[v].short} labelStyle={below} />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="Revenue" value={millions(revenue())} />
          <Figure label="Gross" value={margin(steps()[2].end)} />
          <Figure label="Operating" value={margin(operating())} />
          <Figure label="Net" value={margin(net())} tone="up" />
        </Figures>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 260 : undefined }}>
        <Segmented
          options={BRIDGE_QUARTERS}
          value={quarter()}
          size={size.switch}
          onChange={(next) => {
            if (!reduceMotion()) LayoutAnimation.configureNext(bridgeLayout);
            quarter.set(next);
          }}
        />
      </View>
    </Card>
  );
}

/** An onboarding funnel's stages, by channel: everyone, the app's, the web's. */
const CHANNELS = [
  { value: "all", label: "All" },
  { value: "app", label: "App" },
  { value: "web", label: "Web" },
] as const;
type Channel = (typeof CHANNELS)[number]["value"];
const STAGE_NAMES = ["Visits", "Sign-ups", "KYC verified", "Funded", "First trade"];
const REACHED: Record<Channel, number[]> = {
  app: [96800, 27110, 19240, 10870, 7830],
  web: [87400, 15250, 9170, 4150, 2410],
  all: [184200, 42360, 28410, 15020, 10240],
};
/** A stage: how many reached it, the share of the stage before that did,
 * and its place, the category its color comes from. */
type Stage = { name: string; count: number; kept: number; tier: string };
const stages = (c: Channel): Stage[] =>
  REACHED[c].map((count, i) => ({ name: STAGE_NAMES[i], count, kept: i === 0 ? 1 : count / REACHED[c][i - 1], tier: String(i) }));
/** Half the widest stage, everyone's visits: every channel on the same scale. */
const FUNNEL_HALF = REACHED.all[0] / 2;
const FUNNEL_ROWS = STAGE_NAMES.length;
/** Bars as high as the gaps between them, where the necks run. */
const FUNNEL_PAD = { inner: 0.5, outer: 0.08 };
const FUNNEL_HEIGHT = pointer ? 204 : 216;
/** Where stage `i`'s bar is down the plot, in points (the band scale's own
 * arithmetic): its top, height and middle, and the middle of the gap under it. */
const funnelRow = (i: number) => {
  const band = bandAt(i, FUNNEL_ROWS, FUNNEL_PAD);
  const top = band.start * FUNNEL_HEIGHT;
  const height = band.width * FUNNEL_HEIGHT;
  return { top, height, middle: top + height / 2, gap: top + height + ((band.step - band.width) * FUNNEL_HEIGHT) / 2 };
};
/** The columns beside the bars: each stage's name over its count, and what it keeps. */
const FUNNEL_LEFT = 86;
const FUNNEL_RIGHT = 54;
/** Each stage's depth of the series color, the top stage deepest. */
const TIERS = [1, 0.8, 0.64, 0.5, 0.38];
/** A `#rrggbbaa` color (a theme token as `token` gives it) at `a` of its opacity. */
const fade = (color: string, a: number) => {
  const own = color.length === 9 ? parseInt(color.slice(7), 16) / 255 : 1;
  return color.slice(0, 7) + Math.round(own * a * 255).toString(16).padStart(2, "0");
};
/** How a channel's stages move into another's: the bars on the plot's
 * `motion.update`, the necks' ends on animated values with the same curve. */
const FUNNEL_UPDATE = { duration: 550, easing: "easeInOut" } as const;
const easeInOut = Easing.bezier(0.42, 0, 0.58, 1);
/** The plot's appear and the share of it over which the bars start, top down. */
const FUNNEL_APPEAR = 900;
const FUNNEL_STAGGER = 0.5;
/** When stage `i`'s bar starts to grow. */
const growsAt = (i: number) => (FUNNEL_APPEAR * FUNNEL_STAGGER * i) / (FUNNEL_ROWS - 1);
/** A bar's half width, a share of the plot's width. */
const halfOf = (count: number) => count / (4 * FUNNEL_HALF);
/** Where a stage's bar starts, in points (half of it): a stage of none
 * would be this wide, so a last stage of a few traders stays a bar. */
const FUNNEL_MIN_HALF = 5;
/** The bars' corners, and the necks' edges that follow them. */
const FUNNEL_RADIUS = 4;
/** A stage's half width as a share of a plot `width` wide: from
 * `FUNNEL_MIN_HALF` points for none to the whole width for everyone's
 * visits, in proportion (so a small stage stays a bar and the small ones
 * still differ). */
const halfFor = (count: number, width: number) =>
  width > 2 * FUNNEL_MIN_HALF ? (FUNNEL_MIN_HALF + (width / 2 - FUNNEL_MIN_HALF) * (count / REACHED.all[0])) / width : halfOf(count);
/** A stage's bar's ends on the counts' scale, as `halfFor` sizes it: a
 * new function for each width, so the bars read their rows again. */
const barEnd = (width: number, side: 1 | -1) => (d: Stage) => side * halfFor(d.count, width) * 2 * FUNNEL_HALF;
const percentOf = (v: number) => `${fixed(v * 100, 1)}%`;

/**
 * An onboarding funnel: each stage a bar centered on the middle (a `Bar`
 * from `-count / 2` to `count / 2`, a base per row), deepest at the top
 * and lighter down, and between two stages a soft neck from one bar's
 * ends to the next one's (a `Canvas` under the bars, its ends animated
 * values), so the stages read as one flow. Beside the bars, each stage's
 * name over its count, and what it keeps of the stage before; in the gaps,
 * what is lost there, the biggest drop in the down tone. The bars grow out
 * of the middle top down, the necks and labels fading in after them; a
 * channel moves the bars and the necks together on the same scale. The
 * stage under the pointer or the arrow keys stays lit (the necks, labels
 * and other bars fade) and is read out over the plot.
 */
function Funnel() {
  const channel = signal<Channel>("all");
  const hover = signal<number | null>(null);
  const rows = () => stages(channel());
  const shown = () => (hover() == null ? undefined : rows()[hover()!]);
  const overall = () => rows()[FUNNEL_ROWS - 1].count / rows()[0].count;
  // The stage that keeps the least of the one before it.
  const leak = () => rows().slice(1).reduce((a, b) => (b.kept < a.kept ? b : a));
  const lit = (i: number) => hover() == null || hover() === i;
  const still = reduceMotion();
  // The necks' ends: each bar's half width, moved with the bars.
  const halves = REACHED.all.map((c) => createAnimatedValue(halfOf(c)));
  // The plot's width, for the narrowest stage; the necks' ends follow it
  // at once (a resize moves nothing).
  const plotWidth = signal(0);
  let current: Channel = "all";
  effect(() => {
    const width = plotWidth();
    REACHED[current].forEach((c, i) => halves[i].set(halfFor(c, width)));
  });
  // Each neck in after the bar under it has grown; dimmed with the hover
  // unless it runs into the hovered stage.
  const necksIn = STAGE_NAMES.slice(1).map(() => createAnimatedValue(still ? 1 : 0));
  const necksLit = STAGE_NAMES.slice(1).map(() => createAnimatedValue(1));
  // Each row's labels in once its bar is half grown.
  const labelsIn = STAGE_NAMES.map(() => createAnimatedValue(still ? 1 : 0));
  if (!still) {
    const out = Easing.out(Easing.cubic);
    labelsIn.forEach((v, i) => v.timing(1, { duration: 360, delay: growsAt(i) + 280, easing: out }));
    necksIn.forEach((v, k) => v.timing(1, { duration: 420, delay: growsAt(k + 1) + 360, easing: out }));
  }
  const move = (v: ReturnType<typeof createAnimatedValue>, to: number, options: { duration: number; easing: ReturnType<typeof Easing.out> }) =>
    reduceMotion() ? v.set(to) : v.timing(to, options);
  const onHover = ({ index }: { index: number | null }) => {
    hover.set(index);
    necksLit.forEach((v, k) => move(v, index == null || index === k + 1 ? 1 : 0.35, { duration: 160, easing: Easing.out(Easing.quad) }));
  };
  const switchTo = (next: Channel) => {
    current = next;
    channel.set(next);
    REACHED[next].forEach((c, i) => move(halves[i], halfFor(c, plotWidth()), { duration: FUNNEL_UPDATE.duration, easing: easeInOut }));
  };
  /** A row's fade with the hover. */
  const dim = (i: number) => ({ opacity: lit(i) ? 1 : 0.35 });
  const fading = "transition-opacity duration-150 motion-reduce:transition-none";
  const channelName = () => CHANNELS.find((c) => c.value === channel())!.label;
  return (
    <Card icon="stocks" title="Conversion funnel" subtitle="From a first visit to a first trade, and what each stage keeps">
      <View style={{ gap: 4 }}>
        <Text style={tipTitle}>{(shown()?.name ?? "Visits to first trade").toUpperCase()}</Text>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>
            {shown() ? count(shown()!.count) : percentOf(overall())}
          </Text>
          <Text style={{ flexShrink: 1, fontSize: 12, color: colors.muted, fontVariant: ["tabular-nums"] }}>
            {shown()
              ? hover() === 0
                ? "the top of the funnel"
                : `${percentOf(shown()!.kept)} of ${rows()[hover()! - 1].name.toLowerCase()} · ${percentOf(shown()!.count / rows()[0].count)} of visits`
              : `${count(rows()[FUNNEL_ROWS - 1].count)} traders of ${count(rows()[0].count)} visits`}
          </Text>
        </View>
      </View>
      <View style={{ flexDirection: "row", gap: 10, marginTop: size.gap + 6 }}>
        {/* Each stage's name over its count, end-aligned on its bar's middle. */}
        <View style={{ width: FUNNEL_LEFT, height: FUNNEL_HEIGHT }}>
          {STAGE_NAMES.map((name, i) => (
            <View style={{ position: "absolute", right: 0, left: 0, top: funnelRow(i).middle - 16, height: 32, justifyContent: "center", opacity: labelsIn[i] }}>
              <View class={fading} style={{ alignItems: "flex-end", gap: 2, ...dim(i) }}>
                <Text style={{ numberOfLines: 1, fontSize: 11, color: colors.muted }}>{name}</Text>
                <Text style={{ fontSize: 13, fontWeight: "500", color: colors.text, fontVariant: ["tabular-nums"] }}>{count(rows()[i].count)}</Text>
              </View>
            </View>
          ))}
        </View>
        <View style={{ flex: 1, height: FUNNEL_HEIGHT }} onLayout={({ bounds }) => plotWidth.set(bounds.size.width)}>
          <Plot
            data={rows()}
            // Stages down the plot, the counts across it, centered on zero.
            xScale={{ type: "linear", domain: [-FUNNEL_HALF, FUNNEL_HALF] }}
            yScale={{ type: "band", paddingInner: FUNNEL_PAD.inner, paddingOuter: FUNNEL_PAD.outer }}
            // One hue, deepest at the top stage.
            colorScale={{ type: "ordinal", domain: TIERS.map((_, i) => String(i)), range: TIERS.map((a) => fade(chart.s1, a)) }}
            appearGeneration={1}
            motion={{ appear: { duration: FUNNEL_APPEAR, easing: "easeOut" }, update: FUNNEL_UPDATE, enter: { duration: 150 }, exit: { duration: 150 }, pointer: { stiffness: 320, damping: 32 } }}
            accessibilityLabel={`Conversion funnel, ${channelName()}: ${count(rows()[0].count)} visits to ${count(rows()[FUNNEL_ROWS - 1].count)} first trades, ${percentOf(overall())}`}
            accessibilityValue={{ text: shown() && `${shown()!.name}: ${count(shown()!.count)}, ${percentOf(shown()!.kept)} of the stage before` }}
            crossLineClass="w-full h-full"
            crossLineStyle={{ backgroundColor: alpha(colors.text, 0.045), borderRadius: 6 }}
            onHoverChange={onHover}
            focusStyle={focusRing(4)}
            style={{ height: FUNNEL_HEIGHT }}
          >
            {/* The necks, beside the bars: from one bar's ends to the next one's, curving through the gap. Their
                edges at the bars follow the bars' rounded corners (half a point into them, so no seam shows), so a
                stage and its necks read as one shape. */}
            <Canvas
              style={{ position: "absolute", left: 0, top: 0, right: 0, bottom: 0, pointerEvents: "none" }}
              draw={(cx, { width }) => {
                const mid = width / 2;
                const color = chart.s1;
                const [r, e] = [FUNNEL_RADIUS, 0.5];
                const quarter = Math.PI / 2;
                for (let k = 0; k < FUNNEL_ROWS - 1; k++) {
                  // The bottom of the bar above, the top of the one below.
                  const y0 = funnelRow(k).top + funnelRow(k).height;
                  const y1 = funnelRow(k + 1).top;
                  const ym = (y0 + y1) / 2;
                  const a = halves[k].mul(width);
                  const b = halves[k + 1].mul(width);
                  const [aR, aL, bR, bL] = [a.add(mid), a.mul(-1).add(mid), b.add(mid), b.mul(-1).add(mid)];
                  const wash = cx.createLinearGradient(0, y0, 0, y1);
                  wash.addColorStop(0, fade(color, 0.22 * TIERS[k]));
                  wash.addColorStop(1, fade(color, 0.22 * TIERS[k + 1]));
                  cx.fillStyle = wash;
                  cx.globalAlpha = necksIn[k].mul(necksLit[k]);
                  cx.beginPath();
                  // Along the bar above's bottom, its corners included, right to left.
                  cx.moveTo(aR, y0 - r);
                  cx.arc(aR.sub(r), y0 - r, r - e, 0, quarter);
                  cx.arc(aL.add(r), y0 - r, r - e, quarter, 2 * quarter);
                  cx.lineTo(aL, y0 - r);
                  // Down its left end and through the gap to the bar below.
                  cx.lineTo(aL, y0);
                  cx.bezierCurveTo(aL, ym, bL, ym, bL, y1);
                  cx.lineTo(bL, y1 + r);
                  // Along the bar below's top, left to right.
                  cx.arc(bL.add(r), y1 + r, r - e, 2 * quarter, 3 * quarter);
                  cx.arc(bR.sub(r), y1 + r, r - e, 3 * quarter, 4 * quarter);
                  cx.lineTo(bR, y1 + r);
                  // Up its right end and back.
                  cx.lineTo(bR, y1);
                  cx.bezierCurveTo(bR, ym, aR, ym, aR, y0);
                  cx.closePath();
                  cx.fill();
                }
                cx.globalAlpha = 1;
              }}
            />
            <Bar
              base={barEnd(plotWidth(), -1)}
              value={barEnd(plotWidth(), 1)}
              color="tier"
              cornerRadii={FUNNEL_RADIUS}
              stagger={FUNNEL_STAGGER}
              inactiveOpacity={0.35}
            />
          </Plot>
        </View>
        {/* What each stage keeps of the one before, on its row; what is lost, in the gap above it. */}
        <View style={{ width: FUNNEL_RIGHT, height: FUNNEL_HEIGHT }}>
          {STAGE_NAMES.map((_, i) => (
            <View style={{ position: "absolute", right: 0, top: funnelRow(i).middle - 9, height: 18, opacity: labelsIn[i] }}>
              <View
                class={fading}
                style={{
                  height: 18,
                  justifyContent: "center",
                  paddingHorizontal: 6,
                  borderRadius: 9,
                  backgroundColor: hover() === i ? alpha(chart.s1, 0.22) : alpha(colors.text, 0.06),
                  ...dim(i),
                }}
              >
                <Text style={{ ...axis, color: colors.text }}>{i === 0 ? "100%" : percentOf(rows()[i].kept)}</Text>
              </View>
            </View>
          ))}
          {STAGE_NAMES.slice(1).map((name, k) => (
            <View style={{ position: "absolute", right: 0, top: funnelRow(k).gap - 7, height: 14, justifyContent: "center", opacity: necksIn[k] }}>
              <Text
                class={fading}
                style={{ ...axis, fontSize: 10, paddingRight: 6, color: leak().name === name ? toneColor("down") : colors.faint, ...dim(k + 1) }}
              >
                {`−${percentOf(1 - rows()[k + 1].kept)}`}
              </Text>
            </View>
          ))}
        </View>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="Overall" value={percentOf(overall())} tone="up" />
          <Figure label="Biggest drop" value={leak().name} />
          <Figure label="Lost there" value={`−${percentOf(1 - leak().kept)}`} tone="down" />
        </Figures>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 200 : undefined }}>
        <Segmented options={CHANNELS} value={channel()} size={size.switch} onChange={switchTo} />
      </View>
    </Card>
  );
}

const MONTH_NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** The trading days up to Friday, October 30, 2026: weekdays, oldest first. */
const SESSIONS = (() => {
  const out: string[] = [];
  const day = new Date(Date.UTC(2026, 9, 30));
  while (out.length < 61) {
    const weekday = day.getUTCDay();
    if (weekday !== 0 && weekday !== 6) out.unshift(`${MONTH_NAMES[day.getUTCMonth()]} ${day.getUTCDate()}`);
    day.setUTCDate(day.getUTCDate() - 1);
  }
  return out;
})();
type Candle = { date: string; open: number; high: number; low: number; close: number; volume: number; dir: "up" | "down"; upVolume?: number; downVolume?: number };
const cents = (v: number) => Math.round(v * 100) / 100;
/** A stock's daily candles over those days: a climb, a pullback, a recovery;
 * volume in millions, heavier on the days that move more. */
const CANDLES: Candle[] = (() => {
  const r = random(61);
  let close = 168;
  return SESSIONS.map((date, i) => {
    const open = close * (1 + (r() - 0.5) * 0.012);
    const drift = i < 24 ? 0.0035 : i < 40 ? -0.0045 : 0.004;
    close = open * (1 + drift + (r() - 0.5) * 0.026);
    const high = Math.max(open, close) * (1 + r() * 0.011);
    const low = Math.min(open, close) * (1 - r() * 0.011);
    const volume = Math.round((6 + r() * 6) * (1 + (Math.abs(close - open) / open) * 30) * 100) / 100;
    const dir: Candle["dir"] = close >= open ? "up" : "down";
    const candle: Candle = { date, open: cents(open), high: cents(high), low: cents(low), close: cents(close), volume, dir };
    if (dir === "up") candle.upVolume = volume;
    else candle.downVolume = volume;
    return candle;
  });
})();
const CANDLE_SPANS = [
  { value: "1M", label: "1M", days: 21 },
  { value: "2M", label: "2M", days: 41 },
  { value: "3M", label: "3M", days: 61 },
] as const;
type CandleSpan = (typeof CANDLE_SPANS)[number]["value"];
const CANDLE_PAD = { inner: 0.3, outer: 0.15 };
const PRICE_HEIGHT = 168;
const VOLUME_HEIGHT = 44;

/**
 * Daily candles over their volume: each day a body from its open to its
 * close (a `Bar` with a `base` per row) over a wick from its low to its
 * high (a `Rule`), green up and red down through the plot's `colorScale`,
 * the last close tagged in the gutter at its height. Under it the day's
 * volume in the same bands, in the day's tone. The day under the pointer
 * is marked on both plots and its open, high, low and close read out over
 * them; a period draws its candles in again.
 */
// The x axis's text of a span's days: a new function for each span, so
// the axis writes its labels again with the rows.
const dayNames = (rows: { date: string }[]) => (v: number) => rows[v].date;

function Candles() {
  const span = signal<CandleSpan>("2M");
  const hover = signal<number | null>(null);
  // Which plot the pointer is over: the other one marks the day itself.
  const over = signal<"price" | "volume" | null>(null);
  const rows = () => CANDLES.slice(-CANDLE_SPANS.find((s) => s.value === span())!.days);
  const last = () => rows()[rows().length - 1];
  const shown = () => rows()[hover() != null && hover()! < rows().length ? hover()! : rows().length - 1];
  /** The close before a day's, what its change is measured from. */
  const before = (c: Candle) => {
    const i = CANDLES.indexOf(c);
    return i > 0 ? CANDLES[i - 1].close : c.open;
  };
  const change = () => shown().close - before(shown());
  const tone = (): Tone => (change() >= 0 ? "up" : "down");
  const domain = (): [number, number] => {
    const lo = Math.min(...rows().map((c) => c.low));
    const hi = Math.max(...rows().map((c) => c.high));
    const pad = (hi - lo) * 0.08;
    return [Math.floor(lo - pad), Math.ceil(hi + pad)];
  };
  const grid = () => ticks(domain()[0], domain()[1], 4);
  const y = (v: number) => (1 - (v - domain()[0]) / (domain()[1] - domain()[0])) * PRICE_HEIGHT;
  const topVolume = () => Math.max(...rows().map((c) => c.volume)) * 1.12;
  const total = () => (last().close / rows()[0].open - 1) * 100;
  const ups = () => rows().filter((c) => c.dir === "up").length;
  const dates = () => {
    const n = rows().length - 1;
    return [0, Math.round(n / 3), Math.round((2 * n) / 3), n];
  };
  const follow = (which: "price" | "volume") => ({ index }: { index: number | null }) => {
    hover.set(index);
    over.set(index == null ? null : which);
  };
  // The hovered day's band on the plot the pointer is not over.
  const mark = (which: "price" | "volume") => (
    <If when={hover() != null && over() !== which}>
      <View
        style={{
          position: "absolute",
          left: share(bandAt(hover() ?? 0, rows().length, CANDLE_PAD).start),
          width: share(bandAt(0, rows().length, CANDLE_PAD).width),
          top: 0,
          bottom: 0,
          backgroundColor: alpha(colors.text, 0.06),
          pointerEvents: "none",
        }}
      />
    </If>
  );
  const ohlc = (label: string, v: number) => (
    <Text style={{ ...axis, color: colors.faint }}>
      {`${label} `}
      <Text style={{ color: colors.text }}>{fixed(v)}</Text>
    </Text>
  );
  const motion = { appear: { duration: 800, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 160 } } as const;
  return (
    <Card icon="stocks" title="Daily candles" subtitle="Open to close over each day's range, its volume under it">
      <View style={{ gap: 4 }}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
          <Text style={{ fontSize: 13, fontFamily: mono, color: colors.muted }}>NSHL</Text>
          <Text style={tipTitle}>{shown().date.toUpperCase()}</Text>
        </View>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(shown().close)}</Text>
          <Text style={{ fontSize: 12, fontWeight: "500", color: toneColor(tone()), fontVariant: ["tabular-nums"] }}>
            {`${signed(change())}  ${signed((change() / before(shown())) * 100)}%`}
          </Text>
        </View>
        <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: 12, rowGap: 2 }}>
          {ohlc("O", shown().open)}
          {ohlc("H", shown().high)}
          {ohlc("L", shown().low)}
          {ohlc("C", shown().close)}
          <Text style={{ ...axis, color: colors.faint }}>
            {"VOL "}
            <Text style={{ color: colors.text }}>{`${fixed(shown().volume, 1)}M`}</Text>
          </Text>
        </View>
      </View>
      <View style={{ marginTop: size.gap, gap: 6 }}>
        <Plot
          data={rows()}
          xScale={{ type: "band", paddingInner: CANDLE_PAD.inner, paddingOuter: CANDLE_PAD.outer }}
          yScale={{ type: "linear", domain: domain() }}
          colorScale={{ type: "ordinal", domain: ["up", "down"], range: [colors.up, colors.down] }}
          appearGeneration={span()}
          // No pointer spring: the other plot marks the same day at once, so both move together.
          motion={motion}
          accessibilityLabel={`NSHL daily candles, ${span()}: ${fixed(rows()[0].open)} to ${fixed(last().close)}, ${signed(total())}%`}
          accessibilityValue={{ text: hover() == null ? undefined : `${shown().date}: open ${fixed(shown().open)}, high ${fixed(shown().high)}, low ${fixed(shown().low)}, close ${fixed(shown().close)}` }}
          crossLineClass="w-full h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.06) }}
          onHoverChange={follow("price")}
          focusStyle={focusRing(4)}
          style={{ height: PRICE_HEIGHT, marginRight: GUTTER }}
        >
          <Grid y={grid()} style={{ stroke: colors.line }} />
          {/* The last close: a dotted level across the days. */}
          <Grid y={[last().close]} style={{ stroke: alpha(toneColor(last().dir), 0.55), strokeDasharray: "2 3" }} />
          {/* Wicks under bodies, both in the day's tone. */}
          <Rule value="high" base="low" color="dir" stagger={0.6} inactiveOpacity={0.3} style={{ strokeWidth: 1 }} />
          <Bar value="close" base="open" color="dir" cornerRadii={1} stagger={0.6} inactiveOpacity={0.3} />
          {mark("price")}
          {/* The gutter: the grid's values clear of the last close's tag. */}
          <PlotAxis
            placement="right"
            tickValues={grid().filter((v) => Math.abs(y(v) - y(last().close)) > 14)}
            format={(v) => fixed(v, 0)}
            labelStyle={{ ...axis, paddingLeft: 10 }}
          />
          <PlotAxis
            placement="right"
            tickValues={[last().close]}
            format={(v) => fixed(v)}
            labelStyle={{
              marginLeft: 6,
              paddingHorizontal: 5,
              lineHeight: 18,
              borderRadius: 5,
              backgroundColor: toneColor(last().dir),
              fontSize: 10.5,
              fontFamily: mono,
              fontWeight: "600",
              color: colors.surface,
              fontVariant: ["tabular-nums"],
            }}
          />
        </Plot>
        <Plot
          data={rows()}
          xScale={{ type: "band", paddingInner: CANDLE_PAD.inner, paddingOuter: CANDLE_PAD.outer }}
          yScale={{ type: "linear", domain: [0, topVolume()] }}
          appearGeneration={span()}
          motion={motion}
          accessibilityLabel={`NSHL volume, ${span()}: ${fixed(rows().reduce((s, c) => s + c.volume, 0) / rows().length, 1)} million a day`}
          accessibilityValue={{ text: hover() == null ? undefined : `${shown().date}: ${fixed(shown().volume, 1)} million` }}
          crossLineClass="w-full h-full"
          crossLineStyle={{ backgroundColor: alpha(colors.text, 0.06) }}
          onHoverChange={follow("volume")}
          focusStyle={focusRing(4)}
          style={{ height: VOLUME_HEIGHT, marginRight: GUTTER, marginBottom: BELOW }}
        >
          {/* A day's volume in one of two series, so each takes its tone, faded under the candles'. */}
          <Bar value="upVolume" stack="volume" cornerRadii={{ topLeft: 1, topRight: 1 }} stagger={0.6} inactiveOpacity={0.3} style={{ fill: alpha(colors.up, 0.45) }} />
          <Bar value="downVolume" stack="volume" cornerRadii={{ topLeft: 1, topRight: 1 }} stagger={0.6} inactiveOpacity={0.3} style={{ fill: alpha(colors.down, 0.45) }} />
          <Grid y={[0]} style={{ stroke: colors.border }} />
          {mark("volume")}
          <PlotAxis placement="right" tickValues={[topVolume() / 1.12]} format={(v) => `${fixed(v, 0)}M`} labelStyle={{ ...axis, paddingLeft: 10 }} />
          <PlotAxis tickValues={dates()} format={dayNames(rows())} labelStyle={below} />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="High" value={fixed(Math.max(...rows().map((c) => c.high)))} />
          <Figure label="Low" value={fixed(Math.min(...rows().map((c) => c.low)))} />
          <Figure label="Change" value={`${signed(total(), 1)}%`} tone={total() >= 0 ? "up" : "down"} />
          <Figure label="Up days" value={`${ups()} / ${rows().length}`} />
        </Figures>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 180 : undefined }}>
        <Segmented
          options={CANDLE_SPANS}
          value={span()}
          size={size.switch}
          onChange={(next) => {
            span.set(next);
            hover.set(null);
          }}
        />
      </View>
    </Card>
  );
}

/** A portfolio's lean on six factors, scored 0 to 100, and two benchmarks'. */
const FACTORS = [
  { name: "Value", ours: 72, index: 46, peers: 64 },
  { name: "Momentum", ours: 58, index: 62, peers: 40 },
  { name: "Quality", ours: 86, index: 55, peers: 71 },
  { name: "Size", ours: 34, index: 70, peers: 52 },
  { name: "Low vol", ours: 64, index: 42, peers: 78 },
  { name: "Yield", ours: 48, index: 52, peers: 66 },
];
type Factor = (typeof FACTORS)[number];
const BENCHMARKS = [
  { value: "index", label: "vs Index", name: "Index" },
  { value: "peers", label: "vs Peers", name: "Peers" },
] as const;
type Benchmark = (typeof BENCHMARKS)[number]["value"];
/** The radar's side and its rim: room inside the plot for the hover dots,
 * the factors' names past the rim. */
const RADAR = pointer ? 196 : 204;
const RADAR_RIM = "78%";
/** Where the names sit: a share of the plot's side out from its middle. */
const LABEL_AT = 44;
const mean = (pick: (f: Factor) => number) => FACTORS.reduce((s, f) => s + pick(f), 0) / FACTORS.length;
/** A name's side of its place: away from the center. */
const away = (v: number) => (v > 0.01 ? "flex-start" : v < -0.01 ? "flex-end" : "center");

/**
 * A portfolio's factor exposure as a radar against a benchmark: hexagonal
 * hairline rings and spokes, the benchmark a dashed outline, the portfolio
 * a citrus outline over a wash that deepens toward the rim, growing out of
 * the center and morphing to the other benchmark (`motion.update`). The
 * factor nearest the pointer by angle, or the keys' factor, is read out
 * beside it with both scores and the spread, its name lit at the rim.
 */
function Factors() {
  const bench = signal<Benchmark>("index");
  const hover = signal<number | null>(null);
  const shown = () => (hover() == null ? undefined : FACTORS[hover()!]);
  const benchName = () => BENCHMARKS.find((b) => b.value === bench())!.name;
  const ours = () => shown()?.ours ?? mean((f) => f.ours);
  const theirs = () => (shown() ? shown()![bench()] : mean((f) => f[bench()]));
  const spread = () => ours() - theirs();
  const lead = FACTORS.reduce((a, b) => (b.ours > a.ours ? b : a));
  return (
    <Card icon="stocks" title="Factor exposure" subtitle="A portfolio against its benchmark on six factors">
      {/* The readout beside the radar where the card has room for it, below where it has not. */}
      <View style={{ flexDirection: pointer ? "row" : "column", flexWrap: "wrap", justifyContent: "center", alignItems: "center", gap: pointer ? 20 : 12 }}>
        <Plot
          data={FACTORS}
          yScale={{ type: "linear", domain: [0, 100] }}
          appearGeneration={1}
          motion={{ appear: { duration: 900, easing: "easeOut" }, update: { duration: 550, easing: "easeInOut" }, enter: { duration: 120 }, exit: { duration: 180 }, pointer: { stiffness: 380, damping: 34 } }}
          accessibilityLabel={`Factor exposure against the ${benchName().toLowerCase()}, leaning most on ${lead.name} at ${lead.ours}`}
          accessibilityValue={{ text: shown() && `${shown()!.name}: ${shown()!.ours}, ${benchName().toLowerCase()} ${theirs()}` }}
          onHoverChange={({ index }) => hover.set(index)}
          focusStyle={{ ...focusRing(4), borderRadius: RADAR / 2 }}
          style={{ width: RADAR, height: RADAR, marginHorizontal: 40, marginVertical: 10 }}
        >
          <RadialGrid rings={[25, 50, 75]} outerRadius={RADAR_RIM} style={{ stroke: colors.line }} />
          <RadialGrid rings={[100]} spokes outerRadius={RADAR_RIM} style={{ stroke: colors.border }} />
          <RadialLine
            radius={bench()}
            closed
            outerRadius={RADAR_RIM}
            style={{ stroke: colors.muted, strokeDasharray: "3 3", fill: alpha(colors.muted, 0.06) }}
            hoverDotStyle={{ width: 7, height: 7, borderRadius: 4, borderWidth: 1.5, borderColor: colors.surface, backgroundColor: colors.muted }}
          />
          <RadialLine
            radius="ours"
            closed
            outerRadius={RADAR_RIM}
            dot
            dotStyle={{ width: 4, height: 4, borderRadius: 2, backgroundColor: chart.s1 }}
            backgroundStyle={{ backgroundImage: `radial-gradient(circle, ${alpha(chart.s1, 0.04)}, ${alpha(chart.s1, 0.3)})` }}
            style={{ stroke: chart.s1, strokeWidth: 2 }}
            hoverDotStyle={{ width: 10, height: 10, borderRadius: 5, borderWidth: 2, borderColor: colors.surface, backgroundColor: chart.s1 }}
          />
          {/* Each factor's name past the rim at its angle, growing away from the center; the hovered one lit. */}
          {FACTORS.map((f, i) => {
            const angle = (2 * Math.PI * i) / FACTORS.length;
            const [sin, cos] = [Math.sin(angle), Math.cos(angle)];
            return (
              <View
                style={{
                  position: "absolute",
                  left: `${50 + LABEL_AT * sin}%`,
                  top: `${50 - LABEL_AT * cos}%`,
                  width: 0,
                  height: 0,
                  alignItems: away(sin),
                  justifyContent: away(-cos),
                  pointerEvents: "none",
                }}
              >
                <Text style={{ width: 64, numberOfLines: 1, fontSize: 10.5, fontFamily: mono, textAlign: sin > 0.01 ? "left" : sin < -0.01 ? "right" : "center", color: hover() === i ? chart.s1 : colors.faint }}>
                  {f.name}
                </Text>
              </View>
            );
          })}
        </Plot>
        <View style={{ alignSelf: "stretch", flexGrow: 1, flexBasis: pointer ? 200 : undefined, justifyContent: "center", gap: 12 }}>
          <View style={{ gap: 2 }}>
            <Text style={tipTitle}>{shown()?.name.toUpperCase() ?? "ALL SIX FACTORS"}</Text>
            <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
              <Text style={{ fontSize: 26, fontWeight: "600", letterSpacing: -0.6, color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(ours(), 1)}</Text>
              <Text style={{ fontSize: 12, color: colors.muted }}>{shown() ? "score" : "mean score"}</Text>
            </View>
          </View>
          <View style={{ gap: 6 }}>
            <Legend color={chart.s1} label="Portfolio" value={fixed(ours(), 1)} />
            <Legend color={colors.muted} label={benchName()} value={fixed(theirs(), 1)} />
          </View>
          <Figures>
            <Figure label="Spread" value={signed(spread(), 1)} tone={spread() >= 0 ? "up" : "down"} />
            <Figure label="Lead" value={lead.name} />
          </Figures>
        </View>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 220 : undefined }}>
        <Segmented options={BENCHMARKS} value={bench()} size={size.switch} onChange={(next) => bench.set(next)} />
      </View>
    </Card>
  );
}

/** Two quarters' income, in billions: two segments into revenue, revenue
 * into gross profit and its cost, gross profit into operating income and
 * expenses, operating income into net income and tax. */
const ACCOUNTS = ["Products", "Services", "Revenue", "Gross profit", "Cost", "Operating", "OpEx", "Net income", "Tax"].map((name) => ({ name }));
const PAIRS = [
  [0, 2],
  [1, 2],
  [2, 3],
  [2, 4],
  [3, 5],
  [3, 6],
  [5, 7],
  [5, 8],
];
const QUARTERS = [
  { value: "q3", label: "Q3" },
  { value: "q4", label: "Q4" },
] as const;
type Quarter = (typeof QUARTERS)[number]["value"];
const FLOWED: Record<Quarter, number[]> = {
  q3: [61.6, 23.9, 39.7, 45.8, 23.4, 16.3, 19.9, 3.5],
  q4: [68.2, 25.1, 43.6, 49.7, 27.0, 16.6, 23.1, 3.9],
};
const income = (q: Quarter) => PAIRS.map(([source, target], i) => ({ source, target, value: FLOWED[q][i] }));
/** What flows through account `i`: the most into or out of it. */
const through = (q: Quarter, i: number) => {
  const sum = (end: 0 | 1) => PAIRS.reduce((s, pair, k) => (pair[end] === i ? s + FLOWED[q][k] : s), 0);
  return Math.max(sum(0), sum(1));
};
/** Each account's color: what is kept in the accent, the segments in
 * graphite, what is spent quiet; a ribbon fades from its source's to its
 * target's. */
const NODE = ["fill-chart-2", "fill-chart-3", "fill-chart-1", "fill-chart-1", "fill-chart-4", "fill-chart-1", "fill-chart-4", "fill-chart-1", "fill-chart-4"];
const FROM = ["from-chart-2/30", "from-chart-3/30", "from-chart-1/30", "from-chart-1/30", "from-chart-4/30", "from-chart-1/30", "from-chart-4/30", "from-chart-1/30", "from-chart-4/30"];
const TO = ["to-chart-2/30", "to-chart-3/30", "to-chart-1/30", "to-chart-1/30", "to-chart-4/30", "to-chart-1/30", "to-chart-4/30", "to-chart-1/30", "to-chart-4/30"];
const FLOW_HEIGHT = pointer ? 196 : 220;
/** The room left of the segments' names and right of the results'. */
const FLOW_LEFT = 58;
const FLOW_RIGHT = 64;
/** Columns closer than this break two-word names over two lines, so the
 * names centered over neighbouring columns do not run into each other. */
const FLOW_NARROW = 72;
const billions = (v: number) => `$${fixed(v, 1)}B`;

/**
 * Two quarters' income as flows: each account a hairline node named beside
 * it (left of the segments, right of the results, above the rest), each
 * flow a ribbon fading from its source's color to its target's, what is
 * kept in the accent and what is spent quiet, drawn in from the left. The
 * account under the pointer or the arrow keys is read out over the flows,
 * the links away from it fading; the quarters' flows move into each other.
 */
function Flows() {
  const quarter = signal<Quarter>("q3");
  const hover = signal<number | null>(null);
  const revenue = () => through(quarter(), 2);
  const shown = () => hover() ?? 2;
  const share = (v: number) => `${fixed((v / revenue()) * 100, 1)}%`;
  const margin = (i: number) => share(through(quarter(), i));
  const label = () => QUARTERS.find((q) => q.value === quarter())!.label;
  /** The distance between the five columns, once laid out. */
  const column = signal(Infinity);
  const name = (d: (typeof ACCOUNTS)[number]) => (column() < FLOW_NARROW ? d.name.replace(" ", "\n") : d.name);
  return (
    <Card icon="stocks" title="Flows" subtitle="A quarter's revenue from its segments to net income">
      <View style={{ gap: 4 }}>
        <Text style={tipTitle}>{ACCOUNTS[shown()].name.toUpperCase()}</Text>
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 10 }}>
          <Text style={{ fontSize: 22, fontWeight: "600", letterSpacing: -0.4, color: colors.text, fontVariant: ["tabular-nums"] }}>
            {billions(through(quarter(), shown()))}
          </Text>
          <Text style={{ fontSize: 12, color: colors.muted }}>{`${margin(shown())} of ${label()} revenue`}</Text>
        </View>
      </View>
      <View
        onLayout={({ bounds }) => column.set((bounds.size.width - FLOW_LEFT - FLOW_RIGHT - 3) / 4)}
        style={{ marginTop: size.gap + 18, marginBottom: 6 }}
      >
        <Plot
          data={ACCOUNTS}
          appearGeneration={1}
          motion={{ appear: { duration: 1100, easing: "easeOut" }, update: { duration: 600, easing: "easeInOut" }, enter: { duration: 120 }, exit: { duration: 180 }, pointer: { stiffness: 420, damping: 40 } }}
          accessibilityLabel={`Income flows, ${label()}: ${billions(revenue())} revenue, ${billions(through(quarter(), 7))} net income, a net margin of ${margin(7)}`}
          accessibilityValue={{ text: hover() == null ? undefined : `${ACCOUNTS[hover()!].name}: ${billions(through(quarter(), hover()!))}, ${margin(hover()!)} of revenue` }}
          onHoverChange={({ index }) => hover.set(index)}
          focusStyle={focusRing(4)}
          // Room for the segments' names left of them and the results' right.
          style={{ height: FLOW_HEIGHT, marginLeft: FLOW_LEFT, marginRight: FLOW_RIGHT }}
        >
          <Sankey
            links={income(quarter())}
            nodeAlign="left"
            nodeWidth={3}
            nodePadding={20}
            nodeCornerRadius={1.5}
            hoverDim={0.7}
            nodeClass={(_, i) => NODE[i]}
            linkBackgroundClass={(link) => `bg-linear-to-r ${FROM[link.source]} ${TO[link.target]}`}
            nodeLabel={name}
            nodeLabelStyle={{ ...axis, color: colors.muted, padding: 5, numberOfLines: 2 }}
          />
        </Plot>
      </View>
      <View style={{ marginTop: size.gap + 4 }}>
        <Figures>
          <Figure label="Revenue" value={billions(revenue())} />
          <Figure label="Gross margin" value={margin(3)} />
          <Figure label="Operating" value={margin(5)} />
          <Figure label="Net margin" value={margin(7)} tone="up" />
        </Figures>
      </View>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 160 : undefined }}>
        <Segmented
          options={QUARTERS}
          value={quarter()}
          size={size.switch}
          onChange={(next) => {
            quarter.set(next);
            hover.set(null);
          }}
        />
      </View>
    </Card>
  );
}

/** A watchlist's symbols. */
const SYMBOLS = [
  ["AAPL", "Apple"],
  ["MSFT", "Microsoft"],
  ["NVDA", "NVIDIA"],
  ["AMZN", "Amazon"],
  ["GOOGL", "Alphabet"],
  ["META", "Meta Platforms"],
  ["TSLA", "Tesla"],
  ["AVGO", "Broadcom"],
  ["ORCL", "Oracle"],
  ["AMD", "Advanced Micro Devices"],
  ["NFLX", "Netflix"],
  ["ADBE", "Adobe"],
  ["CRM", "Salesforce"],
  ["QCOM", "Qualcomm"],
  ["INTC", "Intel"],
  ["IBM", "IBM"],
  ["UBER", "Uber"],
  ["SHOP", "Shopify"],
] as const;
/** The watchlist's periods and the closes each shows. */
const SPANS = [
  { value: "1D", label: "1D", points: 39 },
  { value: "1W", label: "1W", points: 35 },
  { value: "1M", label: "1M", points: 22 },
] as const;
type Span = (typeof SPANS)[number]["value"];
type Close = { close: number };
/** A walk of `n` closes from 100, repeatable for its seed. */
function walk(seed: number, n: number): Close[] {
  const next = random(seed);
  let v = 100;
  return Array.from({ length: n }, () => ({ close: (v *= 1 + (next() - 0.49) * 0.024) }));
}
const WATCH = SYMBOLS.map(([symbol, name], i) => ({
  symbol,
  name,
  last: 40 + random(97 * (i + 1))() * 460,
  closes: Object.fromEntries(SPANS.map((p, j) => [p.value, walk(7919 * (i + 1) + 104729 * j, p.points)])) as Record<Span, Close[]>,
}));
type Watch = (typeof WATCH)[number];
/** A row's height; five show at once. */
const WATCH_ROW = 52;

/**
 * A watchlist whose rows a list makes as they scroll in: each row's
 * sparkline draws in from the left the first time it shows for a period,
 * and stays whole when the row scrolls away and back, as the
 * `PlotAppearScope` around the list remembers it; a new period draws the
 * rows in again. The line and its wash in the day's tone, the open a
 * dotted hairline.
 */
function Watchlist() {
  const span = signal<Span>("1D");
  const change = (w: Watch) => {
    const closes = w.closes[span()];
    return (closes[closes.length - 1].close / closes[0].close - 1) * 100;
  };
  return (
    <Card icon="stocks" title="Watchlist" subtitle="Sparklines that draw in once and stay whole as their rows scroll back">
      <PlotAppearScope id="watchlist">
        <List testID="watchlist" each={WATCH} keyExtractor={(w) => w.symbol} estimatedItemSize={WATCH_ROW} style={{ height: WATCH_ROW * 5 }}>
          {(w) => {
            const tone = (): Tone => (change(w()) >= 0 ? "up" : "down");
            const closes = () => w().closes[span()];
            return (
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12, height: WATCH_ROW, borderBottomWidth: 1, borderColor: colors.line }}>
                <View style={{ flex: 1, gap: 2 }}>
                  <Text style={{ fontSize: 13, fontWeight: "600", fontFamily: mono, color: colors.text }}>{w().symbol}</Text>
                  <Text style={{ numberOfLines: 1, fontSize: 11.5, color: colors.faint }}>{w().name}</Text>
                </View>
                <Plot
                  data={closes()}
                  y="close"
                  interactive={false}
                  appearGeneration={`${w().symbol} ${span()}`}
                  motion={{ appear: { duration: 700, easing: "easeOut" } }}
                  accessibilityLabel={`${w().symbol}, ${span()}: ${signed(change(w()))}%`}
                  style={{ width: pointer ? 112 : 88, height: 28 }}
                >
                  <Grid y={[closes()[0].close]} style={{ stroke: colors.line, strokeDasharray: "1 3" }} />
                  <Area backgroundStyle={wash(toneColor(tone()), 0.16)} hoverDotClass="hidden" />
                  <Line style={{ stroke: toneColor(tone()), strokeWidth: 1.5 }} />
                </Plot>
                <View style={{ width: 72, alignItems: "flex-end", gap: 2 }}>
                  <Text style={{ fontSize: 13, fontWeight: "500", color: colors.text, fontVariant: ["tabular-nums"] }}>{fixed(w().last)}</Text>
                  <Text style={{ fontSize: 11.5, color: toneColor(tone()), fontVariant: ["tabular-nums"] }}>{`${signed(change(w()))}%`}</Text>
                </View>
              </View>
            );
          }}
        </List>
      </PlotAppearScope>
      <View style={{ marginTop: size.gap + 4, alignSelf: pointer ? "flex-start" : "stretch", width: pointer ? 180 : undefined }}>
        <Segmented options={SPANS} value={span()} size={size.switch} onChange={(next) => span.set(next)} />
      </View>
    </Card>
  );
}

/**
 * Charts: what an app builds from `nsh/base/plot`'s parts and its own
 * views, with no chart component in between. A live price streamed a tick
 * a second, its newest point breathing and tagged in the gutter; an order
 * book's depth as steps either side of the spread, the side under the
 * pointer read out; a portfolio's value over its underwater plot, the
 * worst run shaded and the hovered day marked on both; capacity rings that
 * sweep in for each period; revenue as a donut whose hovered slice lifts;
 * sales as bars side by side or stacked; bars that span a range of their
 * row (a `base` per row): a quarter's profit bridge from revenue to net
 * profit, each step from where the last one left the total, an
 * onboarding funnel centered on its middle, and daily candles, bodies
 * from open to close over `Rule` wicks from low to high, with their volume
 * under them; a portfolio's factor exposure as a radar against a
 * benchmark, the factor under the pointer read out; a quarter's income
 * as flows from its segments to net income; and a watchlist of
 * sparklines that draw in once. Values on the axes are small
 * monospaced figures the plots' `PlotAxis` parts lay along their edges,
 * grids hairlines, tooltips floating cards with a caps title.
 */
export function Charts() {
  return <DemoPage cards={[Live, Depth, Drawdown, Capacity, Revenue, Sales, Bridge, Funnel, Candles, Factors, Flows, Watchlist]} />;
}
