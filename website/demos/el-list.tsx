import { List, mount, signal, Text, View, type YNode } from "nsh";
import { platform, windowSize } from "nsh/device";
import { tint, ui } from "./palette";

type Trade = { id: string; symbol: string; shares: number; change: number };

const symbols = ["AAPL", "NVDA", "TSLA", "MSFT", "AMZN", "META", "GOOG", "AMD", "NFLX", "INTC"];
const LIMIT = 200;

// A page of made-up trades; ids keep counting so every row has its own key.
let next = 0;
const page = (n: number): Trade[] =>
  Array.from({ length: n }, () => {
    const i = next++;
    return { id: `t${i}`, symbol: symbols[(i * 7) % symbols.length], shares: 10 + ((i * 37) % 290), change: (((i * 53) % 200) - 100) / 10 };
  });

function App() {
  const trades = signal(page(40));
  const loading = signal(false);
  const loads = signal(0);
  const refreshing = signal(false);
  const visible = signal("");
  let list!: YNode;

  const loadMore = () => {
    if (loading() || trades().length >= LIMIT) return;
    loading.set(true);
    setTimeout(() => {
      trades.set([...trades(), ...page(20)]);
      loads.update((n) => n + 1);
      loading.set(false);
    }, 600);
  };

  const columns = () => (windowSize().width >= 700 ? 2 : 1);

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, paddingTop: 24 }}>
      <View style={{ paddingHorizontal: 16, gap: 6, paddingBottom: 8 }}>
        <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>list</Text>
        <Text style={{ color: ui().muted }}>
          {trades().length} rows · onLoadMore {loads()}× · visible {visible()}
        </Text>
      </View>
      <List
        ref={list}
        testID="trades"
        each={trades()}
        keyExtractor={(t) => t.id}
        numColumns={columns()}
        estimatedItemSize={64}
        style={{ flex: 1 }}
        contentStyle={{ paddingHorizontal: 16, paddingTop: 4, paddingBottom: 16, gap: 8 }}
        onLoadMore={loadMore}
        loadMoreThreshold={0.5}
        refreshing={refreshing()}
        onRefresh={() => {
          refreshing.set(true);
          setTimeout(() => {
            next = 0;
            trades.set(page(40));
            loads.set(0);
            refreshing.set(false);
          }, 800);
        }}
        viewabilityConfig={{ itemVisiblePercentThreshold: 50 }}
        onViewableItemsChange={({ viewableItems }) =>
          visible.set(viewableItems.length ? `${viewableItems[0].index}–${viewableItems[viewableItems.length - 1].index}` : "")
        }
        header={
          <View style={{ ...ui().card, borderRadius: 14, padding: 14, gap: 10, marginBottom: 4 }}>
            <Text style={{ color: ui().muted }}>
              {platform.idiom === "desktop" ? "Scroll with the wheel or trackpad" : "Swipe up; pull down to refresh"}: more rows load near the end, up to {LIMIT}.
            </Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              <Chip label="scrollToIndex 100" onPress={() => list.scrollToIndex({ index: Math.min(100, trades().length - 1), viewPosition: 0.5 })} />
              <Chip label="scrollToEnd" onPress={() => list.scrollToEnd({ animated: true })} />
            </View>
          </View>
        }
        footer={
          <Text style={{ color: ui().faint, textAlign: "center", paddingVertical: 16 }}>
            {loading() ? "Loading 20 more…" : trades().length >= LIMIT ? "The end" : ""}
          </Text>
        }
      >
        {(trade, index) => (
          <View style={{ ...ui().row, flex: 1, flexDirection: "row", alignItems: "center", gap: 12, padding: 14, borderRadius: 12 }}>
            <Text style={{ color: ui().faint, width: 32, fontVariant: ["tabular-nums"] }}>{index()}</Text>
            <View style={{ flex: 1 }}>
              <Text style={{ color: ui().text, fontWeight: "600" }}>{trade().symbol}</Text>
              <Text style={{ color: ui().muted, fontSize: 12 }}>
                {trade().shares} shares · key {trade().id}
              </Text>
            </View>
            <Text
              style={{
                color: trade().change >= 0 ? ui().up : ui().down,
                backgroundColor: tint(trade().change >= 0 ? ui().up : ui().down, 0.12),
                paddingHorizontal: 8,
                paddingVertical: 3,
                borderRadius: 6,
                overflow: "hidden",
                fontWeight: "600",
                fontVariant: ["tabular-nums"],
              }}
            >
              {trade().change >= 0 ? "+" : ""}
              {trade().change.toFixed(1)}%
            </Text>
          </View>
        )}
      </List>
    </View>
  );
}

function Chip(props: { label: string; onPress: () => void }) {
  return (
    <View
      onPress={props.onPress}
      activeStyle={{ opacity: 0.6 }}
      style={{ ...ui().raised, paddingHorizontal: 12, paddingVertical: 7, borderRadius: 10 }}
    >
      <Text style={{ color: ui().text, fontWeight: "600" }}>{props.label}</Text>
    </View>
  );
}

mount(App);
