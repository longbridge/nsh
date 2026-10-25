import { List, signal, Text, View } from "nsh";
import { Area, Grid, Line, Plot } from "nsh/base/plot";
import { platform } from "nsh/device";
import { Badge, Button, Caption, Card, colors, DemoPage, mono, toneColor } from "../ui";

const pointer = platform.idiom === "desktop";
const size = pointer ? { gap: 10, label: 14, detail: 12, switch: "small" as const } : { gap: 12, label: 15, detail: 13, switch: "regular" as const };

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/** Visitors per day of two weeks. */
const WEEKS: { day: string; visitors: number }[][] = [
  [1820, 2140, 1960, 2480, 2310, 1290, 1180],
  [1910, 2260, 2390, 2650, 2520, 1420, 1330],
].map((week) => week.map((visitors, i) => ({ day: DAYS[i], visitors })));

/** A repeatable walk of `count` prices from `start`. */
const walk = (count: number, seed: number, start: number) => {
  let price = start;
  let s = seed;
  return Array.from({ length: count }, (_, i) => {
    s = (s * 16807) % 2147483647;
    price += ((s % 1000) / 1000 - 0.48) * start * 0.02;
    return { i, price };
  });
};
const SYMBOLS = ["NSHL", "GPUI", "TAFY", "QJSX", "RUST", "OKLC", "SKIA", "CORE", "BASE", "WASM", "GTK4", "D2D1"];
const TICKERS = Array.from({ length: 48 }, (_, n) => {
  const points = walk(40, 7 + n * 13, 40 + ((n * 37) % 160));
  return { id: n, symbol: `${SYMBOLS[n % SYMBOLS.length]}${n >= SYMBOLS.length ? Math.floor(n / SYMBOLS.length) : ""}`, points };
});

const count = (n: number) => n.toLocaleString("en-US");

/**
 * Charts: `nsh/base/plot`'s `Plot` with `Line`, `Area` and `Grid`, drawn
 * natively from the app's rows: a week of visitors with the hover's cross
 * line, dots and tooltip gliding natively, and a list of sparklines that
 * draw in as their rows come.
 */
export function Charts() {
  const last = signal("—");

  function Visitors() {
    const week = signal(0);
    const hover = signal<number | null>(null);
    const rows = () => WEEKS[week()];
    const shown = () => rows()[hover() ?? rows().length - 1];
    return (
      <Card icon="stocks" title="Visitors" subtitle="A line and an area with hover, grid lines and motion" right={<Badge label={week() === 0 ? "Last week" : "This week"} />}>
        <Caption label={hover() == null ? `${shown().day}, the last day` : shown().day} note={`${count(shown().visitors)} visitors`} />
        <Plot
          data={rows()}
          y="visitors"
          yScale={{ type: "linear", domain: [0, 3000] }}
          appearGeneration={week()}
          motion={{ appear: { duration: 600, easing: "easeOut" }, enter: { duration: 120 }, exit: { duration: 160 }, pointer: { stiffness: 320, damping: 32 } }}
          accessibilityLabel={`Visitors per day, from ${count(rows()[0].visitors)} on Monday to ${count(rows()[6].visitors)} on Sunday`}
          accessibilityValue={{ text: hover() == null ? undefined : `${shown().day}, ${count(shown().visitors)} visitors` }}
          crossLineClass="w-px h-full"
          crossLineStyle={{ backgroundColor: colors.muted }}
          dotClass="size-2.5 rounded-full border-2"
          dotStyle={{ backgroundColor: toneColor("accent"), borderColor: colors.surface }}
          tooltipContent={
            <View style={{ marginTop: 4, paddingHorizontal: 8, paddingVertical: 4, borderRadius: 6, backgroundColor: colors.raised }}>
              <Text style={{ fontSize: 12, color: colors.text, fontVariant: ["tabular-nums"] }}>{count(shown().visitors)}</Text>
            </View>
          }
          onHoverChange={(index) => {
            hover.set(index);
            last.set(index == null ? "Plot: left" : `Plot: ${rows()[index].day}, ${count(rows()[index].visitors)}`);
          }}
          style={{ height: 160, marginTop: size.gap }}
        >
          <Grid y={[0, 1000, 2000, 3000]} style={{ stroke: colors.line }} />
          <Area style={{ fill: toneColor("accent"), opacity: 0.16 }} />
          <Line style={{ stroke: toneColor("accent"), strokeWidth: 2 }} />
        </Plot>
        <View style={{ flexDirection: "row", justifyContent: "space-between", marginTop: 6 }}>
          {DAYS.map((d) => (
            <Text style={{ fontSize: 12, color: colors.muted }}>{d}</Text>
          ))}
        </View>
        <View style={{ flexDirection: "row", marginTop: size.gap, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label={week() === 0 ? "Show this week" : "Show last week"}
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              week.set(1 - week());
              hover.set(null);
              last.set(`Plot: ${week() === 0 ? "last" : "this"} week`);
            }}
          />
        </View>
      </Card>
    );
  }

  function Sparklines() {
    return (
      <Card icon="stocks" title="Sparklines" subtitle="A plot per row of a list, drawn in as the row comes">
        <List each={TICKERS} keyExtractor={(t) => String(t.id)} estimatedItemSize={44} style={{ height: 44 * 6 }}>
          {(ticker) => {
            const up = () => ticker().points[ticker().points.length - 1].price >= ticker().points[0].price;
            return (
              <View style={{ height: 44, flexDirection: "row", alignItems: "center", gap: size.gap }}>
                <Text style={{ width: 64, fontSize: size.label, color: colors.text, fontFamily: mono }}>{ticker().symbol}</Text>
                <Plot
                  data={ticker().points}
                  y="price"
                  interactive={false}
                  appearGeneration={ticker().id}
                  motion={{ appear: { duration: 400, easing: "easeOut" } }}
                  accessibilityLabel={`${ticker().symbol}, ${up() ? "up" : "down"}`}
                  style={{ flex: 1, height: 28 }}
                >
                  <Line style={{ stroke: toneColor(up() ? "up" : "down"), strokeWidth: 1.5 }} />
                </Plot>
                <Text style={{ width: 64, fontSize: size.label, color: colors.muted, fontVariant: ["tabular-nums"], textAlign: "right" }}>
                  {ticker().points[ticker().points.length - 1].price.toFixed(2)}
                </Text>
              </View>
            );
          }}
        </List>
      </Card>
    );
  }

  function Last() {
    return (
      <Card icon="system" title="Last event">
        <Text style={{ fontSize: 13, color: colors.text, fontFamily: mono }}>{last()}</Text>
      </Card>
    );
  }

  return <DemoPage cards={[Visitors, Sparklines, Last]} />;
}
