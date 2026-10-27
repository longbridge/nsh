import { Canvas, type CanvasRenderingContext2D, cleanup, computed, Input, signal, Text, View } from "nsh";
import { Button, Card, colors, Field } from "./ui";

// ─── Counter ────────────────────────────────────────────────────────────

/** Digits five rows tall, drawn with blocks. */
const DIGITS: Record<string, string[]> = {
  "0": ["███", "█ █", "█ █", "█ █", "███"],
  "1": [" █ ", "██ ", " █ ", " █ ", "███"],
  "2": ["███", "  █", "███", "█  ", "███"],
  "3": ["███", "  █", "███", "  █", "███"],
  "4": ["█ █", "█ █", "███", "  █", "  █"],
  "5": ["███", "█  ", "███", "  █", "███"],
  "6": ["███", "█  ", "███", "█ █", "███"],
  "7": ["███", "  █", "  █", "  █", "  █"],
  "8": ["███", "█ █", "███", "█ █", "███"],
  "9": ["███", "█ █", "███", "  █", "███"],
  "-": ["   ", "   ", "███", "   ", "   "],
};

function BigNumber(props: { value: number; color: string }) {
  const rows = () => {
    const glyphs = String(props.value).split("").map((d) => DIGITS[d] ?? DIGITS["0"]);
    return [0, 1, 2, 3, 4].map((row) => glyphs.map((g) => g[row]).join(" ")).join("\n");
  };
  return <Text style={{ color: props.color }}>{rows()}</Text>;
}

export function Counter() {
  const count = signal(0);
  const history = signal<number[]>([]);
  const change = (delta: number) => {
    const next = count() + delta;
    count.set(next);
    history.update((h) => [next, ...h].slice(0, 8));
  };
  const tone = () => (count() < 0 ? colors.down : count() > 0 ? colors.up : colors.text);
  return (
    <>
      <Card title="Value">
        <View style={{ alignItems: "center", gap: 16 }}>
          <BigNumber value={count()} color={tone()} />
          <View style={{ flexDirection: "row", gap: 16 }}>
            <Button label="−10" tone={colors.down} onPress={() => change(-10)} />
            <Button label="−1" tone={colors.down} onPress={() => change(-1)} />
            <Button label="Reset" onPress={() => change(-count())} />
            <Button label="+1" tone={colors.up} onPress={() => change(1)} />
            <Button label="+10" tone={colors.up} onPress={() => change(10)} />
          </View>
        </View>
      </Card>
      <Card title="History">
        <If when={history().length > 0} else={<Text style={{ color: colors.faint }}>Press a button: the values show here.</Text>}>
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 16 }}>
            <Each of={history()}>
              {(value, i) => <Text style={{ color: i() === 0 ? colors.text : colors.muted }}>{i() === 0 ? `▶ ${value()}` : String(value())}</Text>}
            </Each>
          </View>
        </If>
      </Card>
    </>
  );
}

// ─── Watchlist ──────────────────────────────────────────────────────────

interface Quote {
  symbol: string;
  name: string;
  open: number;
  last: number;
  trend: number[];
}

const SPARKS = "▁▂▃▄▅▆▇█";

function sparkline(values: number[]) {
  const low = Math.min(...values);
  const high = Math.max(...values);
  const span = high - low || 1;
  return values.map((v) => SPARKS[Math.round(((v - low) / span) * (SPARKS.length - 1))]).join("");
}

const SEED: [string, string, number][] = [
  ["AAPL", "Apple", 229.87],
  ["MSFT", "Microsoft", 431.2],
  ["NVDA", "NVIDIA", 138.4],
  ["TSLA", "Tesla", 251.6],
  ["AMZN", "Amazon", 187.3],
  ["GOOGL", "Alphabet", 165.1],
  ["META", "Meta Platforms", 575.4],
  ["0700", "Tencent", 412.8],
  ["9988", "Alibaba", 96.35],
  ["BABA", "Alibaba ADR", 98.12],
  ["TSM", "TSMC", 190.7],
  ["AMD", "AMD", 156.9],
];

type SortKey = "symbol" | "last" | "change";

const COLUMNS: { key: SortKey | null; label: string; width: number; right?: boolean }[] = [
  { key: "symbol", label: "Symbol", width: 72 },
  { key: null, label: "Name", width: 144 },
  { key: "last", label: "Last", width: 80, right: true },
  { key: "change", label: "Chg %", width: 80, right: true },
  { key: null, label: "Trend (40 ticks)", width: 336 },
];

export function Watchlist() {
  const quotes = signal<Quote[]>(SEED.map(([symbol, name, open]) => ({ symbol, name, open, last: open, trend: [open] })));
  const sort = signal<{ key: SortKey; descending: boolean }>({ key: "change", descending: true });
  const selected = signal<string | null>(null);
  const running = signal(true);
  const ticks = signal(0);

  const timer = setInterval(() => {
    if (!running()) return;
    ticks.update((t) => t + 1);
    quotes.update((list) =>
      list.map((q) => {
        const last = Math.max(1, q.last * (1 + (Math.random() - 0.5) * 0.008));
        return { ...q, last, trend: [...q.trend, last].slice(-40) };
      }),
    );
  }, 300);
  cleanup(() => clearInterval(timer));

  const change = (q: Quote) => ((q.last - q.open) / q.open) * 100;
  const sorted = computed(() => {
    const { key, descending } = sort();
    const value = (q: Quote) => (key === "symbol" ? q.symbol : key === "last" ? q.last : change(q));
    const list = [...quotes()].sort((a, b) => (value(a) < value(b) ? -1 : value(a) > value(b) ? 1 : 0));
    return descending ? list.reverse() : list;
  });
  const pick = () => quotes().find((q) => q.symbol === selected());
  const toggleSort = (key: SortKey) =>
    sort.update((s) => (s.key === key ? { key, descending: !s.descending } : { key, descending: key !== "symbol" }));
  const advancing = () => quotes().filter((q) => change(q) > 0).length;

  return (
    <>
      <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
        <Button label={running() ? "❚❚ Pause" : "▶ Resume"} onPress={() => running.set(!running())} active={!running()} tone={colors.warn} />
        <Text style={{ color: colors.muted }}>
          tick <Text style={{ color: colors.text }}>{ticks()}</Text> · <Text style={{ color: colors.up }}>{advancing()} up</Text> ·{" "}
          <Text style={{ color: colors.down }}>{quotes().length - advancing()} down</Text> · click a column to sort, a row to select
        </Text>
      </View>
      <Card>
        <View>
          <View style={{ flexDirection: "row", height: 16 }}>
            <Each of={COLUMNS}>
              {(column) => {
                const c = column();
                const arrow = () => (c.key && sort().key === c.key ? (sort().descending ? " ▼" : " ▲") : "");
                return (
                  <View onPress={c.key ? () => toggleSort(c.key!) : undefined} style={{ width: c.width, paddingHorizontal: 8 }}>
                    <Text style={{ fontWeight: "700", color: c.key && sort().key === c.key ? colors.accent : colors.muted, textAlign: c.right ? "right" : "left" }}>
                      {c.label + arrow()}
                    </Text>
                  </View>
                );
              }}
            </Each>
          </View>
          <View style={{ height: 1, backgroundColor: colors.line, marginVertical: 0 }} />
          <Each of={sorted()}>
            {(quote) => {
              const q = () => quote();
              const pct = () => change(q());
              const tone = () => (pct() >= 0 ? colors.up : colors.down);
              const isSelected = () => selected() === q().symbol;
              return (
                <View
                  onPress={() => selected.set(isSelected() ? null : q().symbol)}
                  style={{ flexDirection: "row", height: 16, backgroundColor: isSelected() ? colors.raised : "transparent" }}
                >
                  <Text style={{ width: 72, paddingHorizontal: 8, fontWeight: "700", color: isSelected() ? colors.accent : colors.text }}>{q().symbol}</Text>
                  <Text numberOfLines={1} style={{ width: 144, paddingHorizontal: 8, color: colors.muted }}>
                    {q().name}
                  </Text>
                  <Text style={{ width: 80, paddingHorizontal: 8, textAlign: "right", color: colors.text }}>{q().last.toFixed(2)}</Text>
                  <Text style={{ width: 80, paddingHorizontal: 8, textAlign: "right", color: tone() }}>
                    {(pct() >= 0 ? "+" : "") + pct().toFixed(2) + "%"}
                  </Text>
                  <Text style={{ width: 336, paddingHorizontal: 8, color: tone() }}>{sparkline(q().trend)}</Text>
                </View>
              );
            }}
          </Each>
        </View>
      </Card>
      <If when={pick()}>
        <Card title={`${pick()!.symbol} · ${pick()!.name}`}>
          <View style={{ flexDirection: "row", gap: 32 }}>
            <Text style={{ color: colors.muted }}>
              Open <Text style={{ color: colors.text }}>{pick()!.open.toFixed(2)}</Text>
            </Text>
            <Text style={{ color: colors.muted }}>
              Last <Text style={{ color: colors.text }}>{pick()!.last.toFixed(2)}</Text>
            </Text>
            <Text style={{ color: colors.muted }}>
              High <Text style={{ color: colors.up }}>{Math.max(...pick()!.trend).toFixed(2)}</Text>
            </Text>
            <Text style={{ color: colors.muted }}>
              Low <Text style={{ color: colors.down }}>{Math.min(...pick()!.trend).toFixed(2)}</Text>
            </Text>
          </View>
        </Card>
      </If>
    </>
  );
}

// ─── Form ───────────────────────────────────────────────────────────────

function TextField(props: { value: string; onChange: (v: string) => void; placeholder: string; error?: string; secure?: boolean }) {
  const focused = signal(false);
  return (
    <View style={{ flex: 1, gap: 0 }}>
      <View
        style={{
          borderWidth: 1,
          borderRadius: 8,
          borderColor: props.error ? colors.down : focused() ? colors.accent : colors.line,
          paddingHorizontal: 15,
          paddingVertical: 15,
          backgroundColor: colors.background,
        }}
      >
        <Input
          value={props.value}
          onChange={props.onChange}
          placeholder={props.placeholder}
          masked={props.secure}
          onFocus={() => focused.set(true)}
          onBlur={() => focused.set(false)}
          style={{ height: 16, color: colors.text, placeholderColor: colors.faint }}
        />
      </View>
      <If when={props.error}>
        <Text style={{ color: colors.down }}>✗ {props.error}</Text>
      </If>
    </View>
  );
}

export function Form() {
  const name = signal("");
  const email = signal("");
  const password = signal("");
  const subscribe = signal(true);
  const tried = signal(false);
  const sent = signal<string | null>(null);
  const errors = () => ({
    name: name().trim() ? undefined : "Enter your name",
    email: /^\S+@\S+\.\S+$/.test(email()) ? undefined : "Enter an email like ada@example.com",
    password: password().length >= 6 ? undefined : "At least 6 characters",
  });
  const show = (key: "name" | "email" | "password") => (tried() ? errors()[key] : undefined);
  const strength = () => Math.min(4, Math.floor(password().length / 3));
  const submit = () => {
    tried.set(true);
    const e = errors();
    if (!e.name && !e.email && !e.password) sent.set(`${name()} <${email()}>${subscribe() ? ", subscribed" : ""}`);
  };
  return (
    <>
      <Card title="Sign up">
        <Field label="Name">
          <TextField value={name()} onChange={(v) => name.set(v)} placeholder="Ada Lovelace" error={show("name")} />
        </Field>
        <Field label="Email">
          <TextField value={email()} onChange={(v) => email.set(v)} placeholder="ada@example.com" error={show("email")} />
        </Field>
        <Field label="Password">
          <TextField value={password()} onChange={(v) => password.set(v)} placeholder="6 characters or more" error={show("password")} secure />
        </Field>
        <Field label="Strength">
          <Text style={{ color: [colors.down, colors.down, colors.warn, colors.up, colors.up][strength()] }}>
            {"█".repeat(strength() * 4) + "░".repeat(16 - strength() * 4)}
            <Text style={{ color: colors.muted }}> {["empty", "weak", "fair", "good", "strong"][strength()]}</Text>
          </Text>
        </Field>
        <Field label="Newsletter">
          <View onPress={() => subscribe.set(!subscribe())} role="checkbox" style={{ flexDirection: "row", gap: 8 }}>
            <Text style={{ color: subscribe() ? colors.accent : colors.muted }}>{subscribe() ? "[✓]" : "[ ]"}</Text>
            <Text style={{ color: colors.text }}>Send me the monthly notes</Text>
          </View>
        </Field>
        <View style={{ flexDirection: "row", gap: 16, paddingLeft: 144 }}>
          <Button label="Create account" active tone={colors.accent} onPress={submit} />
          <Button
            label="Clear"
            onPress={() => {
              name.set("");
              email.set("");
              password.set("");
              tried.set(false);
              sent.set(null);
            }}
          />
        </View>
      </Card>
      <If when={sent()}>
        <Card title="Done">
          <Text style={{ color: colors.up }}>✓ Account created for {sent()}</Text>
        </Card>
      </If>
    </>
  );
}

// ─── Controls ───────────────────────────────────────────────────────────

function Switch(props: { value: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <View onPress={() => props.onChange(!props.value)} role="switch" style={{ flexDirection: "row", gap: 16, height: 16 }}>
      <View style={{ width: 48, flexDirection: "row", backgroundColor: props.value ? colors.accent : colors.raised }}>
        <View style={{ width: 16, height: 16, marginLeft: props.value ? 32 : 0, backgroundColor: colors.text }} />
      </View>
      <Text style={{ color: props.value ? colors.text : colors.muted }}>{props.label}</Text>
    </View>
  );
}

function Segmented<T extends string>(props: { options: T[]; value: T; onChange: (v: T) => void }) {
  return (
    <View style={{ flexDirection: "row", gap: 8 }}>
      <Each of={props.options}>
        {(option) => <Button label={option()} active={props.value === option()} onPress={() => props.onChange(option())} />}
      </Each>
    </View>
  );
}

function Bar(props: { value: number; tone: string; width?: number }) {
  const cells = () => (props.width ?? 40);
  const full = () => Math.round(Math.max(0, Math.min(1, props.value)) * cells());
  return (
    <Text>
      <Text style={{ color: props.tone }}>{"█".repeat(full())}</Text>
      <Text style={{ color: colors.raised }}>{"█".repeat(cells() - full())}</Text>
      <Text style={{ color: colors.muted }}> {Math.round(props.value * 100)}%</Text>
    </Text>
  );
}

type Range = "Day" | "Week" | "Month" | "Year";
const RANGES: Range[] = ["Day", "Week", "Month", "Year"];

export function Controls() {
  const wifi = signal(true);
  const bluetooth = signal(false);
  const dark = signal(true);
  const range = signal<Range>("Week");
  const volume = signal(6);
  const plan = signal("Pro");
  const progress = signal(0);
  const dialog = signal(false);
  const timer = setInterval(() => progress.update((p) => (p >= 1 ? 0 : p + 0.02)), 100);
  cleanup(() => clearInterval(timer));
  return (
    <>
      <Card title="Switches">
        <Switch value={wifi()} onChange={(v) => wifi.set(v)} label="Wi-Fi" />
        <Switch value={bluetooth()} onChange={(v) => bluetooth.set(v)} label="Bluetooth" />
        <Switch value={dark()} onChange={(v) => dark.set(v)} label="Dark appearance" />
      </Card>
      <Card title="Choices">
        <Field label="Range">
          <Segmented options={RANGES} value={range()} onChange={(v) => range.set(v)} />
        </Field>
        <Field label="Plan">
          <View style={{ gap: 0 }}>
            <Each of={["Free", "Pro", "Team"]}>
              {(name) => (
                <View onPress={() => plan.set(name())} role="radio" style={{ flexDirection: "row", gap: 8, height: 16 }}>
                  <Text style={{ color: plan() === name() ? colors.accent : colors.muted }}>{plan() === name() ? "(●)" : "( )"}</Text>
                  <Text style={{ color: colors.text }}>{name()}</Text>
                </View>
              )}
            </Each>
          </View>
        </Field>
        <Field label="Volume">
          <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
            <Button label="−" onPress={() => volume.set(Math.max(0, volume() - 1))} />
            <Bar value={volume() / 10} tone={colors.violet} width={20} />
            <Button label="+" onPress={() => volume.set(Math.min(10, volume() + 1))} />
          </View>
        </Field>
      </Card>
      <Card title="Progress">
        <Field label="Download">
          <Bar value={progress()} tone={colors.up} />
        </Field>
        <Field label="Range">
          <Text style={{ color: colors.muted }}>
            Showing the last <Text style={{ color: colors.accent, fontWeight: "700" }}>{range().toLowerCase()}</Text>
          </Text>
        </Field>
        <View style={{ flexDirection: "row" }}>
          <Button label="Open a dialog" active onPress={() => dialog.set(true)} />
        </View>
      </Card>
      <If when={dialog()}>
        <View
          onPress={() => dialog.set(false)}
          style={{ position: "absolute", top: -16, left: -16, right: -16, bottom: -16, backgroundColor: "rgba(0,0,0,0.6)", alignItems: "center", justifyContent: "center" }}
        >
          <View onPress={() => {}} style={{ width: 400 }}>
            <Card title="Delete 3 files?">
              <Text style={{ color: colors.muted }}>The backdrop is black at 60% opacity, blended over the cells below it.</Text>
              <View style={{ flexDirection: "row", gap: 16, justifyContent: "flex-end" }}>
                <Button label="Cancel" onPress={() => dialog.set(false)} />
                <Button label="Delete" active tone={colors.down} onPress={() => dialog.set(false)} />
              </View>
            </Card>
          </View>
        </View>
      </If>
    </>
  );
}

// ─── Text ───────────────────────────────────────────────────────────────

const LONG =
  "Native Shell lays text out once, on the engine thread: the terminal's text engine counts cells instead of glyph advances, breaking lines at spaces and inside words too long for a line, so the core's flexbox sizes every text exactly to the grid.";

export function TextDemo() {
  return (
    <>
      <Card title="Styles">
        <Text style={{ color: colors.text }}>
          <Text style={{ fontWeight: "700" }}>Bold</Text> · <Text style={{ fontStyle: "italic" }}>italic</Text> ·{" "}
          <Text style={{ textDecorationLine: "underline" }}>underline</Text> ·{" "}
          <Text style={{ textDecorationLine: "line-through", color: colors.muted }}>struck</Text> ·{" "}
          <Text style={{ color: colors.accent }}>blue</Text> <Text style={{ color: colors.up }}>green</Text>{" "}
          <Text style={{ color: colors.warn }}>amber</Text> <Text style={{ color: colors.down }}>red</Text>{" "}
          <Text style={{ color: colors.violet }}>violet</Text>
        </Text>
        <Text style={{ color: colors.text }}>
          Wide characters take two cells: <Text style={{ color: colors.accent }}>你好，终端！こんにちは 안녕하세요</Text>
        </Text>
      </Card>
      <Card title="Wrapping">
        <Text style={{ color: colors.muted }}>{LONG}</Text>
      </Card>
      <Card title="numberOfLines={1} and {2}">
        <Text numberOfLines={1} style={{ color: colors.text }}>
          {LONG}
        </Text>
        <Text numberOfLines={2} style={{ color: colors.muted }}>
          {LONG}
        </Text>
      </Card>
      <Card title="Alignment">
        <Text style={{ color: colors.text, textAlign: "left" }}>left ◀</Text>
        <Text style={{ color: colors.text, textAlign: "center" }}>▶ center ◀</Text>
        <Text style={{ color: colors.text, textAlign: "right" }}>▶ right</Text>
      </Card>
    </>
  );
}

// ─── Layout ─────────────────────────────────────────────────────────────

function Box(props: { label: string; color: string; flex?: number; width?: number }) {
  return (
    <View style={{ flex: props.flex, width: props.width, height: 48, backgroundColor: props.color, alignItems: "center", justifyContent: "center" }}>
      <Text style={{ color: colors.background, fontWeight: "700" }}>{props.label}</Text>
    </View>
  );
}

export function Layout() {
  const justify = signal<"flex-start" | "center" | "flex-end" | "space-between" | "space-around">("space-between");
  return (
    <>
      <Card title="flex: 1 · 2 · 1">
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Box label="1" color={colors.accent} flex={1} />
          <Box label="2" color={colors.violet} flex={2} />
          <Box label="1" color={colors.up} flex={1} />
        </View>
      </Card>
      <Card title={`justifyContent: ${justify()}`}>
        <View style={{ flexDirection: "row", gap: 8, flexWrap: "wrap" }}>
          <Each of={["flex-start", "center", "flex-end", "space-between", "space-around"] as const}>
            {(j) => <Button label={j()} active={justify() === j()} onPress={() => justify.set(j())} />}
          </Each>
        </View>
        <View style={{ flexDirection: "row", justifyContent: justify(), backgroundColor: colors.background }}>
          <Box label="A" color={colors.warn} width={80} />
          <Box label="B" color={colors.down} width={80} />
          <Box label="C" color={colors.up} width={80} />
        </View>
      </Card>
      <Card title="Nesting and borders">
        <View style={{ flexDirection: "row", gap: 16 }}>
          <View style={{ flex: 1, borderWidth: 1, borderColor: colors.accent, paddingHorizontal: 8, paddingVertical: 15 }}>
            <Text style={{ color: colors.accent }}>Square corners</Text>
          </View>
          <View style={{ flex: 1, borderWidth: 1, borderRadius: 8, borderColor: colors.violet, paddingHorizontal: 8, paddingVertical: 15 }}>
            <Text style={{ color: colors.violet }}>Rounded corners</Text>
          </View>
          <View style={{ flex: 1, borderLeftWidth: 1, borderColor: colors.up, paddingHorizontal: 16, backgroundColor: colors.raised }}>
            <Text style={{ color: colors.up }}>A left rule</Text>
            <Text style={{ color: colors.muted }}>quote-like</Text>
          </View>
        </View>
      </Card>
    </>
  );
}

// ─── Long list ──────────────────────────────────────────────────────────

const WORDS = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"];

export function LongList() {
  const items = Array.from({ length: 500 }, (_, i) => ({
    id: i + 1,
    title: `${WORDS[i % WORDS.length]} ${WORDS[(i * 7) % WORDS.length]}`,
    size: ((i * 7919) % 9000) + 120,
  }));
  const done = signal<Set<number>>(new Set());
  const toggle = (id: number) =>
    done.update((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  return (
    <>
      <Text style={{ color: colors.muted }}>
        500 rows in one scroll view: wheel, ↑ ↓, PgUp / PgDn. Click a row to tick it ·{" "}
        <Text style={{ color: colors.up }}>{done().size} ticked</Text>
      </Text>
      <View>
        <Each of={items}>
          {(item) => {
            const it = item();
            const ticked = () => done().has(it.id);
            return (
              <View onPress={() => toggle(it.id)} style={{ flexDirection: "row", height: 16, gap: 16, backgroundColor: it.id % 2 ? "transparent" : colors.surface }}>
                <Text style={{ width: 48, textAlign: "right", color: colors.faint }}>{String(it.id)}</Text>
                <Text style={{ color: ticked() ? colors.up : colors.muted }}>{ticked() ? "✓" : "·"}</Text>
                <Text style={{ flex: 1, color: ticked() ? colors.muted : colors.text, textDecorationLine: ticked() ? "line-through" : "none" }}>{it.title}</Text>
                <Text style={{ width: 96, textAlign: "right", paddingRight: 16, color: colors.faint }}>{(it.size / 100).toFixed(1)} KB</Text>
              </View>
            );
          }}
        </Each>
      </View>
    </>
  );
}

// ─── Scope ──────────────────────────────────────────────────────────────

/** A signal of a few harmonics drifting over time, as two channels. */
function sample(t: number, phase: number, channel: 0 | 1) {
  const f = channel === 0 ? 1 : 1.5;
  return (
    0.55 * Math.sin(2 * Math.PI * f * t + phase) +
    0.25 * Math.sin(2 * Math.PI * 3 * f * t + phase * 1.7) +
    0.12 * Math.sin(2 * Math.PI * 7 * t - phase * 2.3)
  );
}

function Grid(cx: CanvasRenderingContext2D, width: number, height: number) {
  cx.strokeStyle = colors.line;
  cx.beginPath();
  cx.moveTo(0, height / 2);
  cx.lineTo(width, height / 2);
  cx.moveTo(width / 2, 0);
  cx.lineTo(width / 2, height);
  cx.stroke();
}

export function Scope() {
  const phase = signal(0);
  const running = signal(true);
  const timer = setInterval(() => running() && phase.update((p) => p + 0.12), 50);
  cleanup(() => clearInterval(timer));
  return (
    <>
      <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
        <Button label={running() ? "❚❚ Pause" : "▶ Live"} onPress={() => running.set(!running())} active={!running()} tone={colors.warn} />
        <Text style={{ color: colors.muted }}>
          Drawn on a <Text style={{ color: colors.text }}>{"<Canvas>"}</Text> with paths: ratatui's canvas in braille dots, 2 × 4 to a cell
        </Text>
      </View>
      <Card title="oscilloscope · amplitude">
        <Canvas
          style={{ height: 192 }}
          draw={(cx, { width, height }) => {
            Grid(cx, width, height);
            const p = phase();
            for (const [channel, color] of [[0, colors.down], [1, colors.accent]] as const) {
              cx.strokeStyle = color;
              cx.beginPath();
              for (let x = 0; x <= width; x += 2) {
                const y = height / 2 - sample(x / width, p, channel) * (height / 2 - 8);
                if (x === 0) cx.moveTo(x, y);
                else cx.lineTo(x, y);
              }
              cx.stroke();
            }
            cx.fillStyle = colors.faint;
            cx.fillText("time →", width - 56, height - 4);
          }}
        />
      </Card>
      <View style={{ flexDirection: "row", gap: 16 }}>
        <Card title="spectroscope · level" grow>
          <Canvas
            style={{ height: 176 }}
            draw={(cx, { width, height }) => {
              const p = phase();
              const bars = Math.floor(width / 8);
              for (let i = 0; i < bars; i++) {
                const f = i / bars;
                const peak = Math.exp(-((f - 0.12) ** 2) / 0.002) + 0.7 * Math.exp(-((f - 0.38 - 0.05 * Math.sin(p)) ** 2) / 0.001);
                const level = Math.min(1, 0.08 + peak * (0.8 + 0.2 * Math.sin(p * 3 + i)) + 0.05 * Math.random());
                const h = level * (height - 16);
                cx.fillStyle = level > 0.6 ? colors.down : level > 0.3 ? colors.warn : colors.violet;
                cx.fillRect(i * 8 + 1, height - h, 4, h);
              }
              cx.fillStyle = colors.faint;
              cx.fillText("frequency →", width - 96, 12);
            }}
          />
        </Card>
        <Card title="vectorscope · left / right">
          <Canvas
            style={{ width: 352, height: 176 }}
            draw={(cx, { width, height }) => {
              Grid(cx, width, height);
              const p = phase();
              cx.strokeStyle = colors.up;
              cx.beginPath();
              for (let i = 0; i <= 400; i++) {
                const t = i / 400;
                const x = width / 2 + sample(t, p, 0) * (width / 2 - 8);
                const y = height / 2 - sample(t, p * 0.7, 1) * (height / 2 - 8);
                if (i === 0) cx.moveTo(x, y);
                else cx.lineTo(x, y);
              }
              cx.stroke();
              cx.strokeStyle = colors.violet;
              cx.beginPath();
              cx.arc(width / 2, height / 2, 24 + 8 * Math.sin(p), 0, Math.PI * 2);
              cx.stroke();
            }}
          />
        </Card>
      </View>
    </>
  );
}
