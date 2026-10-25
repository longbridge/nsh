import { cleanup, signal, Text, View } from "nsh";
import { DataTable, type DataTableColumn, type DataTableSort } from "nsh/base";

// Live quotes in `nsh/base`'s `DataTable`, as gpui-component's data table
// story: thousands of stocks of forty columns, a tick every 33 ms that
// moves a third of the quotes. The app formats only the rows in view (and a
// reach around them) and hands those over (`data`, `dataStart`,
// `rowCount`), so a tick costs what is on screen, whatever the number of
// stocks. Rises and falls take their colors natively (`positiveClass`,
// `negativeClass`), with no call into JS per cell.

const TICK_MS = 33;
const REACH = 40;
const COUNTERS = [
  ["AAPL", "Apple", "US"],
  ["MSFT", "Microsoft", "US"],
  ["NVDA", "NVIDIA", "US"],
  ["TSLA", "Tesla", "US"],
  ["AMZN", "Amazon", "US"],
  ["GOOG", "Alphabet", "US"],
  ["META", "Meta Platforms", "US"],
  ["0700.HK", "Tencent", "HK"],
  ["9988.HK", "Alibaba", "HK"],
  ["3690.HK", "Meituan", "HK"],
  ["1810.HK", "Xiaomi", "HK"],
  ["600519.SH", "Kweichow Moutai", "CN"],
  ["300750.SZ", "CATL", "CN"],
  ["D05.SI", "DBS Group", "SG"],
] as const;

// A stock's numbers, by field.
const F = {
  price: 0,
  change: 1,
  percent: 2,
  volume: 3,
  turnover: 4,
  cap: 5,
  ttm: 6,
  bid: 7,
  bidVolume: 8,
  ask: 9,
  askVolume: 10,
  open: 11,
  prevClose: 12,
  high: 13,
  low: 14,
  shares: 15,
  ratio: 16,
  ranking: 17,
} as const;
const FIELDS = 18;

const compact = (v: number) => {
  const a = Math.abs(v);
  if (a >= 1e12) return `${(v / 1e12).toFixed(2)}T`;
  if (a >= 1e9) return `${(v / 1e9).toFixed(2)}B`;
  if (a >= 1e6) return `${(v / 1e6).toFixed(2)}M`;
  if (a >= 1e3) return `${(v / 1e3).toFixed(2)}K`;
  return v.toFixed(2);
};
const signed = (v: number, digits = 2, unit = "") => `${v > 0 ? "+" : ""}${v.toFixed(digits)}${unit}`;

type Stock = { id: number; symbol: string; name: string; market: string; n: Float64Array };

function tick(s: Stock) {
  const n = s.n;
  const percent = (Math.random() - 0.5) * 0.2;
  const price = 5 + Math.random() * 994;
  n[F.price] = price;
  n[F.percent] = percent;
  n[F.change] = price * percent;
  n[F.prevClose] = price - n[F.change];
  n[F.volume] = 1e4 + Math.random() * 5e7;
  n[F.turnover] = n[F.volume] * price;
  n[F.cap] = price * n[F.shares];
  n[F.ttm] = 5 + Math.random() * 75;
  n[F.bid] = price * (1 - Math.random() * 0.01);
  n[F.bidVolume] = 100 + Math.random() * 5e4;
  n[F.ask] = price * (1 + Math.random() * 0.01);
  n[F.askVolume] = 100 + Math.random() * 5e4;
  n[F.ratio] = n[F.bidVolume] / n[F.askVolume];
  n[F.high] = price * (1 + Math.random() * 0.08);
  n[F.low] = price * (1 - Math.random() * 0.08);
  n[F.ranking] = n[F.ranking] * (1 + (Math.random() - 0.5) * 0.4);
}

function makeStocks(count: number): Stock[] {
  return Array.from({ length: count }, (_, id) => {
    const [symbol, name, market] = COUNTERS[id % COUNTERS.length];
    const n = new Float64Array(FIELDS);
    n[F.shares] = 1e8 + Math.random() * 1e10;
    n[F.open] = 5 + Math.random() * 994;
    n[F.ranking] = 1 + Math.random() * 100;
    const stock = { id, symbol, name, market, n };
    tick(stock);
    return stock;
  });
}

const right = { align: "end" as const, width: 110 };
const tone = { positiveClass: "text-green-600", negativeClass: "text-red-600" };
const columns: DataTableColumn[] = [
  { key: "id", title: "ID", width: 70, align: "end", fixed: true },
  { key: "symbol", title: "Symbol", width: 110, fixed: true },
  { key: "name", title: "Name", width: 160 },
  { key: "market", title: "Market", width: 80 },
  { key: "price", title: "Price", ...right, sortable: true },
  { key: "change", title: "Chg", ...right, ...tone, sortable: true },
  { key: "percent", title: "Chg%", ...right, ...tone, sortable: true },
  { key: "volume", title: "Volume", ...right, sortable: true },
  { key: "turnover", title: "Turnover", ...right },
  { key: "cap", title: "Market Cap", ...right, sortable: true },
  { key: "ttm", title: "TTM", ...right },
  { key: "bid", title: "Bid", ...right },
  { key: "bidVolume", title: "Bid Vol", ...right },
  { key: "ask", title: "Ask", ...right },
  { key: "askVolume", title: "Ask Vol", ...right },
  { key: "open", title: "Open", ...right },
  { key: "prevClose", title: "Prev Close", ...right },
  { key: "high", title: "High", ...right },
  { key: "low", title: "Low", ...right },
  { key: "ratio", title: "Bid/Ask", ...right },
  { key: "ranking", title: "5m Ranking", ...right },
  ...Array.from({ length: 20 }, (_, i) => ({
    key: `x${i + 1}`,
    title: `Indicator ${i + 1}`,
    ...right,
    ...tone,
  })),
];

/** The cells of a stock's row, in the columns' order. */
function row(s: Stock): (string | number)[] {
  const n = s.n;
  const out: (string | number)[] = [
    s.id + 1,
    s.symbol,
    s.name,
    s.market,
    n[F.price].toFixed(2),
    signed(n[F.change]),
    signed(n[F.percent] * 100, 2, "%"),
    compact(n[F.volume]),
    compact(n[F.turnover]),
    compact(n[F.cap]),
    n[F.ttm].toFixed(2),
    n[F.bid].toFixed(2),
    compact(n[F.bidVolume]),
    n[F.ask].toFixed(2),
    compact(n[F.askVolume]),
    n[F.open].toFixed(2),
    n[F.prevClose].toFixed(2),
    n[F.high].toFixed(2),
    n[F.low].toFixed(2),
    n[F.ratio].toFixed(2),
    n[F.ranking].toFixed(1),
  ];
  for (let i = 0; i < 20; i++) out.push(signed(n[(i + 1) % FIELDS] * (i % 2 ? 0.01 : -0.01) * n[F.percent] * 100));
  return out;
}

const SORT_FIELD: Record<string, number> = {
  price: F.price,
  change: F.change,
  percent: F.percent,
  volume: F.volume,
  cap: F.cap,
};

/**
 * Live quotes: `count` stocks in a `DataTable`, the rows in view ticking
 * every 33 ms while `live`.
 */
export function QuotesTable(props: { count: number; live: boolean; testID?: string }) {
  const stocks = makeStocks(props.count);
  let order = stocks.map((_, i) => i);
  let visible = { start: 0, end: 40 };
  const sort = signal<DataTableSort | undefined>(undefined);
  const page = signal<{ start: number; rows: (string | number)[][] }>({ start: 0, rows: [] });
  const ticks = signal(0);

  const serve = () => {
    const start = Math.max(0, visible.start - REACH);
    const end = Math.min(order.length, visible.end + REACH);
    const rows = [];
    for (let i = start; i < end; i++) rows.push(row(stocks[order[i]]));
    page.set({ start, rows });
  };

  const step = () => {
    if (!props.live) return;
    // A third of the rows in view (and around them) move, as in
    // gpui-component's story; the rest of the stocks wait to be shown.
    const start = Math.max(0, visible.start - REACH);
    const end = Math.min(order.length, visible.end + REACH);
    for (let i = start; i < end; i++) {
      if (i % (3 + Math.floor(Math.random() * 7)) === 0) tick(stocks[order[i]]);
    }
    serve();
    ticks.set(ticks() + 1);
  };
  const timer = setInterval(step, TICK_MS);
  cleanup(() => clearInterval(timer));
  serve();

  return (
    <View class="flex-1 gap-2">
      <Text class="font-mono text-[12px] text-muted-foreground">
        {`${props.count.toLocaleString("en-US")} stocks · ${columns.length} columns · tick ${ticks()}`}
      </Text>
      <DataTable
        testID={props.testID}
        class="flex-1"
        columns={columns}
        data={page().rows}
        dataStart={page().start}
        rowCount={stocks.length}
        rowHeight={28}
        fontSize={13}
        stripe
        sort={sort()}
        onSortChange={(next) => {
          sort.set(next ?? undefined);
          const field = next ? SORT_FIELD[next.key] : undefined;
          order = stocks.map((_, i) => i);
          if (next && field !== undefined) {
            const sign = next.direction === "ascending" ? 1 : -1;
            order.sort((a, b) => sign * (stocks[a].n[field] - stocks[b].n[field]));
          }
          serve();
        }}
        onVisibleRowsChange={(range) => {
          visible = range;
          serve();
        }}
      />
    </View>
  );
}
