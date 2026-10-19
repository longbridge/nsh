// The nsh app examples/embed-gpui runs, several times at once, inside a GPUI
// Kit window. It sets no theme of its own: the components of nsh/ui/mobile
// take the host's (`AppView::set_theme`), in its light or dark mode
// (`AppView::set_appearance`), and change with it.
//
// The host provides the rest by name, at run time, so this app builds with
// the stock `nsh build`: its desk (`nativeModule("host/trading")`, Rust
// functions the host registers) and two GPUI Kit widgets (`nativeComponent`).
// The desk is the host's: every ticket subscribes to it, so a symbol picked
// or an order placed here shows in the host and in the other tickets.

import { effect, mount, signal, Text, View } from "nsh";
import { nativeComponent, nativeModule } from "nsh/native";
import { Button, Card, Group, Stepper, TextField } from "nsh/ui/mobile";

type Side = "buy" | "sell";
interface Order { id: number; side: Side; symbol: string; quantity: number; price: number; extended: boolean }
interface Position { symbol: string; shares: number }
/** The host's desk: what the host and every ticket show. */
interface Desk { symbol: string; price: number; orders: Order[]; positions: Position[] }

/** `host/trading`'s `Trading` (the host's `src/trading.rs`). */
interface TradingApi {
  /** The sample price of `symbol`; throws for symbols the desk does not trade. */
  quote(symbol: string): number;
  /** Selects `symbol` in the host and in every ticket. */
  select(symbol: string): void;
  /** Checks an order in Rust and adds it to the desk; throws when it is not valid. */
  placeOrder(symbol: string, quantity: number, side: Side, extended: boolean): Order;
  /** Calls `onChange` with the desk now and after every change. */
  subscribe(onChange: (desk: Desk) => void): Promise<void>;
}

const { Trading } = nativeModule<{ Trading: TradingApi }>("host/trading");

/** The host app's switch: a GPUI Kit widget it registers (`host.switch`). */
const HostSwitch = nativeComponent<{ label: string; checked: boolean }, { change: { checked: boolean } }>("host.switch", {
  props: ["label", "checked"],
  events: ["change"],
});

/** The host app's watchlist: GPUI Kit buttons it registers (`host.watchlist`). */
const HostWatchlist = nativeComponent<{ symbol: string }, { select: { symbol: string } }>("host.watchlist", {
  props: ["symbol"],
  events: ["select"],
});

const desk = signal<Desk>({ symbol: "AAPL", price: Trading.quote("AAPL"), orders: [], positions: [] });
Trading.subscribe((d) => desk.set(d));

const known = (symbol: string) => {
  try {
    Trading.quote(symbol);
    return true;
  } catch {
    return false;
  }
};

function App() {
  const draft = signal(desk().symbol);
  const extended = signal(false);
  const quantity = signal(10);
  const error = signal("");
  /** The orders this ticket placed. */
  const mine = signal<number[]>([]);
  // The field follows the desk's symbol, wherever it was picked.
  effect(() => draft.set(desk().symbol));

  function edit(value: string) {
    const symbol = value.toUpperCase();
    draft.set(symbol);
    if (known(symbol)) Trading.select(symbol);
  }

  function place(side: Side) {
    try {
      const order = Trading.placeOrder(draft(), quantity(), side, extended());
      mine.update((ids) => [...ids, order.id]);
      error.set("");
    } catch (e) {
      error.set((e as Error).message);
    }
  }

  const held = () => desk().positions.find((p) => p.symbol === desk().symbol)?.shares ?? 0;
  const muted = { opacity: 0.7 };
  return (
    <View style={{ flex: 1, padding: 16, gap: 12 }}>
      <Card header={<Text style={{ fontSize: 17, fontWeight: "600" }}>Order ticket</Text>}>
        <HostWatchlist
          symbol={desk().symbol}
          onSelect={(e) => Trading.select(e.symbol)}
          style={{ height: 32, alignSelf: "flex-start" }}
        />
        <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
          <Text style={{ fontSize: 22, fontWeight: "600" }}>{`$${desk().price.toFixed(2)}`}</Text>
          <Text style={muted}>{`${desk().symbol} · held ${held()}`}</Text>
        </View>
        <TextField label="Symbol" value={draft()} onChange={edit} clearable />
        <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <Text>Quantity</Text>
          <Stepper value={quantity()} min={1} max={1000} onChange={(v) => quantity.set(v)} />
        </View>
        <HostSwitch
          label="Extended hours"
          checked={extended()}
          onChange={(e) => extended.set(e.checked)}
          style={{ alignSelf: "flex-start" }}
        />
        {error() ? <Text accessibilityRole="alert" style={{ fontWeight: "600" }}>{error()}</Text> : null}
        <View style={{ flexDirection: "row", gap: 8 }}>
          <View style={{ flex: 1 }}>
            <Button title="Buy" onPress={() => place("buy")} />
          </View>
          <View style={{ flex: 1 }}>
            <Button title="Sell" variant="danger" onPress={() => place("sell")} />
          </View>
        </View>
      </Card>
      <Group header={<Text style={muted}>Desk orders, all tickets</Text>}>
        {desk().orders.length === 0 ? (
          <Text style={muted}>None yet</Text>
        ) : (
          desk().orders.slice(0, 4).map((o) => (
            <Text>
              {`#${o.id} ${o.side === "buy" ? "Buy" : "Sell"} ${o.quantity} ${o.symbol}${o.extended ? " (extended)" : ""} · ${
                mine().includes(o.id) ? "this ticket" : "another ticket"
              }`}
            </Text>
          ))
        )}
      </Group>
    </View>
  );
}

mount(App);
