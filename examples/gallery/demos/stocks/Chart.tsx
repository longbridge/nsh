import { computed, effect, signal, Text, View } from "nsh";
import { createAnimatedValue } from "nsh/animated";
import { Canvas, type CanvasRenderingContext2D } from "nsh/canvas";
import { colors, Legend, radius, Segmented } from "../../ui";
import { type Candle, compact, OPEN, price, SESSION, type Stock, signed, time, toneOf } from "./market";
import { alpha, calm, dashed, grid, hatch, pulse, scrub } from "./parts";

export type Period = "intraday" | "1m" | "5m" | "15m" | "1D";
const periods: { value: Period; label: string }[] = [
  { value: "intraday", label: "Intraday" },
  { value: "1m", label: "1m" },
  { value: "5m", label: "5m" },
  { value: "15m", label: "15m" },
  { value: "1D", label: "1D" },
];
const averages = [
  {
    n: 5,
    get color() {
      return colors.accent;
    },
  },
  {
    n: 10,
    get color() {
      return colors.info;
    },
  },
  {
    n: 20,
    get color() {
      return colors.warn;
    },
  },
];
/** Candle bodies (points) by period. */
const bodies: Record<Period, number> = { intraday: 1, "1m": 5, "5m": 7, "15m": 9, "1D": 5 };
const pad = { left: 0, right: 54, top: 8, bottom: 18 };
const HEIGHT = 216;

/** The candles of a period: minutes merged into 5 / 15, or the days with
 * today last. */
function candles(stock: Stock, period: Period): Candle[] {
  const minutes = stock.minutes();
  if (period === "1m" || period === "intraday") return minutes;
  if (period === "1D") {
    const q = stock.quote();
    const today = { t: 0, o: q.open, h: q.high, l: q.low, c: q.last, v: q.volume, a: q.turnover / q.volume };
    return [...stock.days, today];
  }
  const span = period === "5m" ? 300 : 900;
  const out: Candle[] = [];
  for (const m of minutes) {
    const t = m.t - ((m.t - OPEN) % span);
    const last = out[out.length - 1];
    if (last && last.t === t) {
      last.h = Math.max(last.h, m.h);
      last.l = Math.min(last.l, m.l);
      last.c = m.c;
      last.v += m.v;
      last.a = m.a;
    } else {
      out.push({ ...m, t });
    }
  }
  return out;
}

/** The mean close of the `n` candles ending at `i` (null before there are `n`). */
function average(data: Candle[], i: number, n: number) {
  if (i < n - 1) return null;
  let sum = 0;
  for (let k = i - n + 1; k <= i; k++) sum += data[k].c;
  return sum / n;
}

/** A polyline through `n` points. */
function line(ctx: CanvasRenderingContext2D, n: number, x: (i: number) => number, y: (i: number) => number) {
  ctx.beginPath();
  ctx.moveTo(x(0), y(0));
  for (let i = 1; i < n; i++) ctx.lineTo(x(i), y(i));
}

const label = (c: Candle, period: Period) => (period === "1D" ? (c.t === 0 ? "Today" : `${c.t}d ago`) : time(c.t, false));

/** The geometry of one drawing, shared with the crosshair. */
interface Frame {
  first: number;
  count: number;
  lo: number;
  hi: number;
  mainH: number;
  volTop: number;
  volH: number;
  plotW: number;
}

/**
 * The stock's chart: the intraday time-share line with its average price,
 * or candles with MA5 / 10 / 20, both over volume. Drag sideways for the
 * crosshair; its lines move natively and the readout floats beside it.
 * The plot is 216 pt tall unless `height` says otherwise.
 */
export function Chart(props: { stock: Stock; running: () => boolean; height?: number }) {
  const period = signal<Period>("intraday");
  const cursor = signal<number | null>(null);
  const width = signal(0);
  const live = pulse(props.running);
  // The live point's ring, built once: redraws reuse the same animated nodes.
  const ringAlpha = live.interpolate({ inputRange: [0, 1], outputRange: [0.45, 0] });
  const ringRadius = live.interpolate({ inputRange: [0, 1], outputRange: [3, 13] });
  // The minutes are one array mutated in place: every run is a change.
  const data = computed(() => candles(props.stock, period()), { equals: false });
  const step = () => bodies[period()] + 2;
  const intraday = () => period() === "intraday";
  /** Minutes the intraday axis spans: the session so far (an hour at
   * least) and a narrow margin ahead, never past the close. */
  const span = () => Math.min(SESSION, Math.ceil(Math.max(60, data().length) * 1.06));
  /** The candles shown: today's minutes intraday, else the newest that fit. */
  const view = () => {
    if (intraday()) return { first: 0, count: span() };
    const count = Math.max(1, Math.floor((width() - pad.left - pad.right) / step()));
    return { first: Math.max(0, data().length - count), count };
  };
  const X = (i: number) => {
    const plotW = width() - pad.left - pad.right;
    if (intraday()) return pad.left + (i / span()) * plotW;
    return pad.left + (i - view().first) * step() + step() / 2;
  };
  let frame: Frame | null = null;
  // Redrawn on every tick: about a second to settle after the data narrows.
  const scale = calm(0.06);
  effect(() => {
    period();
    scale.reset();
  });

  // The crosshair: two hairlines and a dot placed by animated values.
  const cx = createAnimatedValue(0);
  const cy = createAnimatedValue(0);
  const shown = createAnimatedValue(0);
  const pick = (x: number | null) => {
    if (x === null) {
      cursor.set(null);
      shown.timing(0, { duration: 180 });
      return;
    }
    const d = data();
    const plotW = width() - pad.left - pad.right;
    const { first } = view();
    const raw = intraday() ? Math.round(((x - pad.left) / plotW) * span()) : first + Math.floor((x - pad.left) / step());
    const i = Math.max(first, Math.min(d.length - 1, raw));
    if (cursor() === null) shown.timing(1, { duration: 140 });
    cursor.set(i);
    cx.set(X(i));
    if (frame) cy.set(pad.top + ((frame.hi - d[i].c) / (frame.hi - frame.lo)) * frame.mainH);
  };
  const focus = () => {
    const d = data();
    return d[cursor() ?? d.length - 1];
  };
  const p = (v: number) => price(v, props.stock);
  const prevClose = () => props.stock.quote().prevClose;
  /** The readout's left edge: beside the cursor, flipped near the right. */
  const readoutLeft = () => {
    const i = cursor();
    if (i === null) return 0;
    const x = X(i);
    return x > width() / 2 ? Math.max(0, x - 150) : Math.min(width() - 140, x + 10);
  };

  const drawIntraday = (ctx: CanvasRenderingContext2D, plotW: number, f: Frame) => {
    const d = data();
    const Y = (v: number) => pad.top + ((f.hi - v) / (f.hi - f.lo)) * f.mainH;
    const now = X(d.length - 1);
    const last = d[d.length - 1].c;
    const tone = toneOf(last - prevClose());
    hatch(ctx, now + 4, pad.top, pad.left + plotW - now - 4, f.mainH, grid(), 7);
    dashed(ctx, pad.left, pad.left + plotW, Y(prevClose()), colors.faint);
    // The price area and line.
    const fill = ctx.createLinearGradient(0, pad.top, 0, pad.top + f.mainH);
    fill.addColorStop(0, alpha(tone, 0.26));
    fill.addColorStop(1, alpha(tone, 0));
    ctx.beginPath();
    ctx.moveTo(X(0), pad.top + f.mainH);
    for (let i = 0; i < d.length; i++) ctx.lineTo(X(i), Y(d[i].c));
    ctx.lineTo(now, pad.top + f.mainH);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.lineJoin = "round";
    line(ctx, d.length, X, (i) => Y(d[i].c));
    ctx.strokeStyle = tone;
    ctx.lineWidth = 1.5;
    ctx.stroke();
    // The day's average price.
    line(ctx, d.length, X, (i) => Y(d[i].a));
    ctx.strokeStyle = colors.warn;
    ctx.lineWidth = 1.1;
    ctx.stroke();
    // Volume.
    const vw = Math.max(1, plotW / span() - 0.8);
    let vmax = 0;
    for (const c of d) vmax = Math.max(vmax, c.v);
    for (let i = 0; i < d.length; i++) {
      const c = d[i];
      const h = (c.v / (vmax || 1)) * f.volH;
      ctx.fillStyle = alpha(c.c >= c.o ? colors.up : colors.down, 0.6);
      ctx.fillRect(X(i) - vw / 2, f.volTop + f.volH - h, vw, h);
    }
    // The live point, pulsing natively.
    const ly = Y(last);
    ctx.fillStyle = tone;
    ctx.globalAlpha = ringAlpha;
    ctx.beginPath();
    ctx.arc(now, ly, ringRadius, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.beginPath();
    ctx.arc(now, ly, 3, 0, Math.PI * 2);
    ctx.fill();
    // The open, the middle and the end of the span.
    ctx.fillStyle = colors.faint;
    ctx.textBaseline = "top";
    for (const [k, align] of [[0, "left"], [0.5, "center"], [1, "right"]] as const) {
      ctx.textAlign = align;
      ctx.fillText(time(OPEN + Math.round(k * span()) * 60, false), pad.left + k * plotW, f.volTop + f.volH + 5);
    }
  };

  const drawCandles = (ctx: CanvasRenderingContext2D, f: Frame) => {
    const d = data();
    const Y = (v: number) => pad.top + ((f.hi - v) / (f.hi - f.lo)) * f.mainH;
    const body = bodies[period()];
    const end = Math.min(d.length, f.first + f.count);
    let vmax = 0;
    for (let i = f.first; i < end; i++) vmax = Math.max(vmax, d[i].v);
    for (let i = f.first; i < end; i++) {
      const c = d[i];
      const x = X(i);
      const color = c.c >= c.o ? colors.up : colors.down;
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, Y(c.h));
      ctx.lineTo(x, Y(c.l));
      ctx.stroke();
      const top = Y(Math.max(c.o, c.c));
      const h = Math.max(1, Y(Math.min(c.o, c.c)) - top);
      ctx.fillStyle = c.c >= c.o ? colors.inset : color;
      ctx.fillRect(x - body / 2, top, body, h);
      if (c.c >= c.o) ctx.strokeRect(x - body / 2 + 0.5, top + 0.5, body - 1, Math.max(0, h - 1));
      const vh = (c.v / (vmax || 1)) * f.volH;
      ctx.fillStyle = alpha(color, 0.5);
      ctx.fillRect(x - body / 2, f.volTop + f.volH - vh, body, vh);
    }
    ctx.lineWidth = 1.2;
    ctx.lineJoin = "round";
    for (const a of averages) {
      ctx.strokeStyle = a.color;
      ctx.beginPath();
      let started = false;
      for (let i = f.first; i < end; i++) {
        const v = average(d, i, a.n);
        if (v === null) continue;
        if (started) ctx.lineTo(X(i), Y(v));
        else ctx.moveTo(X(i), Y(v));
        started = true;
      }
      ctx.stroke();
    }
    ctx.fillStyle = colors.faint;
    ctx.textBaseline = "top";
    const n = end - f.first;
    for (const [k, align] of [[0, "left"], [Math.floor(n / 2), "center"], [n - 1, "right"]] as const) {
      ctx.textAlign = align;
      ctx.fillText(label(d[f.first + k], period()), Math.min(pad.left + f.plotW, Math.max(pad.left, X(f.first + k))), f.volTop + f.volH + 5);
    }
  };

  return (
    <View style={{ gap: 10 }}>
      <Segmented options={periods} value={period()} onChange={(v) => period.set(v)} size="small" />
      <View style={{ flexDirection: "row", flexWrap: "wrap", columnGap: 14, rowGap: 6 }}>
        <If
          when={intraday()}
          else={
            <Each of={averages}>
              {(a) => {
                const v = () => average(data(), cursor() ?? data().length - 1, a().n);
                return <Legend color={a().color} label={`MA${a().n}`} value={v() === null ? "–" : p(v()!)} />;
              }}
            </Each>
          }
        >
          <Legend color={toneOf(focus().c - prevClose())} label="Price" value={p(focus().c)} />
          <Legend color={colors.warn} label="Avg" value={p(focus().a)} />
          <Legend color={colors.faint} label="Vol" value={compact(focus().v)} />
        </If>
      </View>
      <View style={{ height: props.height ?? HEIGHT }}>
        <Canvas
          style={{ height: props.height ?? HEIGHT }}
          onLayout={(e) => width.set(e.width)}
          {...scrub(pick)}
          accessibilityLabel={`${props.stock.code} ${period()} chart`}
          draw={(ctx, { width, height }) => {
            const d = data();
            const { first, count } = view();
            const end = Math.min(d.length, first + count);
            if (end <= first) return;
            const plotW = width - pad.left - pad.right;
            const volH = 40;
            const mainH = height - pad.top - pad.bottom - volH - 12;
            const volTop = pad.top + mainH + 12;
            let lo = intraday() ? prevClose() : Infinity, hi = intraday() ? prevClose() : -Infinity;
            for (let i = first; i < end; i++) {
              lo = Math.min(lo, d[i].l);
              hi = Math.max(hi, d[i].h);
            }
            const margin = (hi - lo) * 0.08 || hi * 0.01;
            // Intraday the range is the day's, which only widens; candles
            // scroll, so their range eases in rather than jumping.
            const range = intraday() ? { lo: lo - margin, hi: hi + margin } : scale.fit(lo - margin, hi + margin);
            const f: Frame = { first, count, lo: range.lo, hi: range.hi, mainH, volTop, volH, plotW };
            frame = f;
            const Y = (v: number) => pad.top + ((f.hi - v) / (f.hi - f.lo)) * mainH;

            // The grid and the price axis (percent from the close intraday).
            ctx.font = "11px system-ui";
            ctx.textBaseline = "middle";
            ctx.textAlign = "left";
            ctx.lineWidth = 1;
            for (let k = 0; k <= 4; k++) {
              const v = f.hi - ((f.hi - f.lo) * k) / 4;
              const y = Y(v);
              ctx.strokeStyle = grid();
              ctx.beginPath();
              ctx.moveTo(pad.left, y);
              ctx.lineTo(pad.left + plotW, y);
              ctx.stroke();
              ctx.fillStyle = colors.faint;
              ctx.textAlign = "right";
              ctx.fillText(intraday() ? `${signed(((v - prevClose()) / prevClose()) * 100)}%` : p(v), width, y);
            }
            ctx.strokeStyle = grid();
            ctx.beginPath();
            ctx.moveTo(pad.left, volTop + volH);
            ctx.lineTo(pad.left + plotW, volTop + volH);
            ctx.stroke();

            if (intraday()) drawIntraday(ctx, plotW, f);
            else drawCandles(ctx, f);

            // The last price on the axis.
            const last = d[d.length - 1].c;
            const ly = Math.max(pad.top + 8, Math.min(pad.top + mainH - 8, Y(last)));
            const tone = toneOf(intraday() ? last - prevClose() : last - d[d.length - 1].o);
            if (!intraday()) dashed(ctx, pad.left, pad.left + plotW, ly, alpha(tone, 0.7), [3, 3]);
            ctx.fillStyle = tone;
            ctx.beginPath();
            ctx.roundRect(pad.left + plotW + 2, ly - 9, pad.right - 2, 18, 5);
            ctx.fill();
            ctx.fillStyle = colors.inset;
            ctx.textAlign = "right";
            ctx.textBaseline = "middle";
            ctx.font = "600 11px system-ui";
            ctx.fillText(p(last), width - 4, ly);
          }}
        />
        {/* The crosshair over the canvas, moved by animated values. */}
        <View pointerEvents="none" style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, opacity: shown }}>
          <View
            style={{
              position: "absolute",
              top: pad.top,
              bottom: pad.bottom,
              left: 0,
              width: 1,
              backgroundColor: colors.muted,
              transform: [{ translateX: cx }],
            }}
          />
          <View
            style={{
              position: "absolute",
              left: pad.left,
              right: pad.right,
              top: 0,
              height: 1,
              backgroundColor: alpha(colors.muted, 0.6),
              transform: [{ translateY: cy }],
            }}
          />
          <View
            style={{
              position: "absolute",
              top: -4,
              left: -4,
              width: 9,
              height: 9,
              borderRadius: 4.5,
              backgroundColor: colors.text,
              borderWidth: 2,
              borderColor: colors.inset,
              transform: [{ translateX: cx }, { translateY: cy }],
            }}
          />
          <If when={cursor() !== null}>
            <View
              style={{
                position: "absolute",
                top: pad.top + 2,
                left: readoutLeft(),
                width: 140,
                padding: 10,
                gap: 3,
                borderRadius: radius.control,
                borderCurve: "continuous",
                backgroundColor: alpha(colors.surface, 0.94),
                borderWidth: 1,
                borderColor: colors.border,
              }}
            >
              <Text style={{ fontSize: 12, color: colors.muted, fontVariant: ["tabular-nums"] }}>{label(focus(), period())}</Text>
              <Text style={{ fontSize: 17, fontWeight: "600", color: colors.text, fontVariant: ["tabular-nums"] }}>
                {p(focus().c)}
                <Text style={{ fontSize: 12, fontWeight: "400", fontVariant: ["tabular-nums"], color: toneOf(focus().c - prevClose()) }}>
                  {" "}
                  {signed(((focus().c - prevClose()) / prevClose()) * 100)}%
                </Text>
              </Text>
              <If
                when={intraday()}
                else={
                  <Text style={{ fontSize: 11, lineHeight: 15, color: colors.muted, fontVariant: ["tabular-nums"] }}>
                    O {p(focus().o)} H {p(focus().h)}
                    {"\n"}L {p(focus().l)} V {compact(focus().v)}
                  </Text>
                }
              >
                <Text style={{ fontSize: 11, color: colors.muted, fontVariant: ["tabular-nums"] }}>
                  avg {p(focus().a)} · vol {compact(focus().v)}
                </Text>
              </If>
            </View>
          </If>
        </View>
      </View>
    </View>
  );
}
