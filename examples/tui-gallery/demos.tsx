import { Canvas, type CanvasRenderingContext2D, cleanup, computed, signal, Text, View } from "nsh";
import { Input } from "nsh/base";
import { Button, colors, Field, Rule, Section, VerticalRule } from "./ui";

/** A level drawn as a line, `━━━━────`: filled in its tone, the rest faint. */
function Meter(props: { value: number; tone: string; cells: number; suffix: string }) {
  const filled = () => Math.round(Math.max(0, Math.min(1, props.value)) * props.cells);
  return (
    <Text>
      <Text style={{ color: props.tone }}>{"━".repeat(filled())}</Text>
      <Text style={{ color: colors.faint }}>{"─".repeat(props.cells - filled())}</Text>
      <Text style={{ color: colors.muted }}> {props.suffix}</Text>
    </Text>
  );
}

// ─── Counter ────────────────────────────────────────────────────────────

/** Digits five rows tall, drawn with block characters in the text color. */
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

function BigNumber(props: { value: number }) {
  const rows = () => {
    const glyphs = String(props.value).split("").map((d) => DIGITS[d] ?? DIGITS["0"]);
    return [0, 1, 2, 3, 4].map((row) => glyphs.map((g) => g[row]).join(" ")).join("\n");
  };
  return <Text style={{ color: colors.text }}>{rows()}</Text>;
}

export function Counter() {
  const count = signal(0);
  const history = signal<number[]>([]);
  const change = (delta: number) => {
    const next = count() + delta;
    count.set(next);
    history.update((h) => [next, ...h].slice(0, 8));
  };
  const sign = () => (count() < 0 ? "below zero" : count() > 0 ? "above zero" : "zero");
  const tone = () => (count() < 0 ? colors.down : count() > 0 ? colors.up : colors.muted);
  return (
    <>
      <Section title="Value">
        <BigNumber value={count()} />
        <Text style={{ color: tone() }}>{sign()}</Text>
        <View style={{ flexDirection: "row", gap: 8, marginTop: 16 }}>
          <Button label="−10" onPress={() => change(-10)} />
          <Button label="−1" onPress={() => change(-1)} />
          <Button label="Reset" onPress={() => change(-count())} />
          <Button label="+1" onPress={() => change(1)} />
          <Button label="+10" onPress={() => change(10)} />
        </View>
      </Section>
      <Section title="History">
        <If when={history().length > 0} else={<Text style={{ color: colors.faint }}>Press a button: the values show here.</Text>}>
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 16 }}>
            <Each of={history()}>
              {(value, i) => <Text style={{ color: i() === 0 ? colors.accent : colors.muted }}>{i() === 0 ? `▸ ${value()}` : String(value())}</Text>}
            </Each>
          </View>
        </If>
      </Section>
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

/** The selection marker's column, before the others. */
const MARKER_WIDTH = 16;

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
      <View style={{ flexDirection: "row", gap: 16 }}>
        <Button label={running() ? "Pause" : "Resume"} onPress={() => running.set(!running())} active={!running()} />
        <Text style={{ color: colors.muted }}>
          tick <Text style={{ color: colors.text }}>{ticks()}</Text> · <Text style={{ color: colors.up }}>{advancing()} up</Text> ·{" "}
          <Text style={{ color: colors.down }}>{quotes().length - advancing()} down</Text> · click a column to sort, a row to select
        </Text>
      </View>
      <View>
        <View style={{ flexDirection: "row", height: 16 }}>
          <View style={{ width: MARKER_WIDTH }} />
          <Each of={COLUMNS}>
            {(column) => {
              const c = column();
              const isSortKey = () => c.key !== null && sort().key === c.key;
              const arrow = () => (isSortKey() ? (sort().descending ? " ▼" : " ▲") : "");
              return (
                <View focusStyle={{ backgroundColor: colors.focus }} onPress={c.key ? () => toggleSort(c.key!) : undefined} style={{ width: c.width, paddingHorizontal: 8 }}>
                  <Text style={{ fontWeight: "700", color: isSortKey() ? colors.accent : colors.text, textAlign: c.right ? "right" : "left" }}>
                    {c.label + arrow()}
                  </Text>
                </View>
              );
            }}
          </Each>
        </View>
        <Rule />
        <Each of={sorted()}>
          {(quote) => {
            const q = () => quote();
            const pct = () => change(q());
            const tone = () => (pct() >= 0 ? colors.up : colors.down);
            const isSelected = () => selected() === q().symbol;
            return (
              <View focusStyle={{ backgroundColor: colors.focus }} onPress={() => selected.set(isSelected() ? null : q().symbol)} style={{ flexDirection: "row", height: 16 }}>
                <Text style={{ width: MARKER_WIDTH, color: colors.accent }}>{isSelected() ? "▸" : " "}</Text>
                <Text style={{ width: 72, paddingHorizontal: 8, fontWeight: "700", color: isSelected() ? colors.accent : colors.text }}>{q().symbol}</Text>
                <Text style={{ numberOfLines: 1, width: 144, paddingHorizontal: 8, color: isSelected() ? colors.accent : colors.muted }}>
                  {q().name}
                </Text>
                <Text style={{ width: 80, paddingHorizontal: 8, textAlign: "right", color: isSelected() ? colors.accent : colors.text }}>{q().last.toFixed(2)}</Text>
                <Text style={{ width: 80, paddingHorizontal: 8, textAlign: "right", color: tone() }}>
                  {(pct() >= 0 ? "+" : "") + pct().toFixed(2) + "%"}
                </Text>
                <Text style={{ width: 336, paddingHorizontal: 8, color: tone() }}>{sparkline(q().trend)}</Text>
              </View>
            );
          }}
        </Each>
      </View>
      <If when={pick()}>
        <Section title={`${pick()!.symbol} · ${pick()!.name}`}>
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
        </Section>
      </If>
    </>
  );
}

// ─── Form ───────────────────────────────────────────────────────────────

/** A one-row field between brackets, as Emacs's widgets: `[ text ]`, the error on the row below. */
function TextField(props: { value: string; onChange: (v: string) => void; placeholder: string; error?: string; secure?: boolean }) {
  const focused = signal(false);
  const bracket = () => (props.error ? colors.down : focused() ? colors.accent : colors.faint);
  return (
    <View style={{ flex: 1 }}>
      <View style={{ flexDirection: "row", height: 16 }}>
        <Text style={{ color: bracket() }}>[ </Text>
        <Input
          value={props.value}
          onChange={props.onChange}
          placeholder={props.placeholder}
          masked={props.secure}
          onFocus={() => focused.set(true)}
          onBlur={() => focused.set(false)}
          style={{ flex: 1, height: 16, color: colors.text, placeholderColor: colors.faint }}
        />
        <Text style={{ color: bracket() }}> ]</Text>
      </View>
      <If when={props.error}>
        <Text style={{ color: colors.down }}>✗ {props.error}</Text>
      </If>
    </View>
  );
}

const STRENGTH_WORDS = ["empty", "weak", "fair", "good", "strong"];

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
      <Section title="Sign up">
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
          <Meter
            value={strength() / 4}
            cells={16}
            tone={[colors.down, colors.down, colors.warn, colors.up, colors.up][strength()]}
            suffix={STRENGTH_WORDS[strength()]}
          />
        </Field>
        <Field label="Newsletter">
          <View focusStyle={{ backgroundColor: colors.focus }} onPress={() => subscribe.set(!subscribe())} role="checkbox" style={{ flexDirection: "row", gap: 8 }}>
            <Text style={{ color: subscribe() ? colors.accent : colors.muted }}>{subscribe() ? "[x]" : "[ ]"}</Text>
            <Text style={{ color: colors.text }}>Send me the monthly notes</Text>
          </View>
        </Field>
        <View style={{ flexDirection: "row", gap: 8, paddingLeft: 144, marginTop: 16 }}>
          <Button label="Create account" active onPress={submit} />
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
      </Section>
      <If when={sent()}>
        <Text style={{ color: colors.up }}>✓ Account created for {sent()}</Text>
      </If>
    </>
  );
}

// ─── Controls ───────────────────────────────────────────────────────────

/** A switch as text: `[on ]` in the accent, `[off]` faint. */
function Switch(props: { value: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <View focusStyle={{ backgroundColor: colors.focus }} onPress={() => props.onChange(!props.value)} role="switch" style={{ flexDirection: "row", gap: 8, height: 16 }}>
      <Text style={{ fontWeight: props.value ? "700" : "400", color: props.value ? colors.accent : colors.faint }}>
        {props.value ? "[on ]" : "[off]"}
      </Text>
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
      <Section title="Switches">
        <Switch value={wifi()} onChange={(v) => wifi.set(v)} label="Wi-Fi" />
        <Switch value={bluetooth()} onChange={(v) => bluetooth.set(v)} label="Bluetooth" />
        <Switch value={dark()} onChange={(v) => dark.set(v)} label="Dark appearance" />
      </Section>
      <Section title="Choices">
        <Field label="Range">
          <Segmented options={RANGES} value={range()} onChange={(v) => range.set(v)} />
        </Field>
        <Field label="Plan">
          <View>
            <Each of={["Free", "Pro", "Team"]}>
              {(name) => (
                <View focusStyle={{ backgroundColor: colors.focus }} onPress={() => plan.set(name())} role="radio" style={{ flexDirection: "row", gap: 8, height: 16 }}>
                  <Text style={{ color: plan() === name() ? colors.accent : colors.faint }}>{plan() === name() ? "(*)" : "( )"}</Text>
                  <Text style={{ color: plan() === name() ? colors.text : colors.muted }}>{name()}</Text>
                </View>
              )}
            </Each>
          </View>
        </Field>
        <Field label="Volume">
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Button label="−" onPress={() => volume.set(Math.max(0, volume() - 1))} />
            <Meter value={volume() / 10} tone={colors.accent} cells={20} suffix={`${volume() * 10}%`} />
            <Button label="+" onPress={() => volume.set(Math.min(10, volume() + 1))} />
          </View>
        </Field>
      </Section>
      <Section title="Progress">
        <Field label="Download">
          <Meter value={progress()} tone={colors.up} cells={40} suffix={`${Math.round(progress() * 100)}%`} />
        </Field>
        <Field label="Range">
          <Text style={{ color: colors.muted }}>
            Showing the last <Text style={{ color: colors.accent, fontWeight: "700" }}>{range().toLowerCase()}</Text>
          </Text>
        </Field>
        <View style={{ flexDirection: "row", marginTop: 16 }}>
          <Button label="Open a dialog" onPress={() => dialog.set(true)} />
        </View>
      </Section>
      <If when={dialog()}>
        <View
          onPress={() => dialog.set(false)}
          style={{ position: "absolute", top: -16, left: -8, right: -8, bottom: -16, alignItems: "center", justifyContent: "center" }}
        >
          {/* The one fill of the app: the dialog has to hide the rows under it. */}
          <View
            onPress={() => {}}
            style={{ width: 400, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.page, paddingHorizontal: 15, paddingVertical: 15 }}
          >
            <Text style={{ fontWeight: "700", color: colors.text }}>Delete 3 files?</Text>
            <Text style={{ color: colors.muted }}>The box is drawn with line characters over the page; click outside it to cancel.</Text>
            <View style={{ flexDirection: "row", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
              <Button label="Cancel" onPress={() => dialog.set(false)} />
              <Button label="Delete" tone={colors.down} onPress={() => dialog.set(false)} />
            </View>
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
      <Section title="Styles">
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
      </Section>
      <Section title="Wrapping">
        <Text style={{ color: colors.muted }}>{LONG}</Text>
      </Section>
      <Section title="numberOfLines={1} and {2}">
        <Text style={{ numberOfLines: 1, color: colors.text }}>
          {LONG}
        </Text>
        <Text style={{ numberOfLines: 2, color: colors.muted }}>
          {LONG}
        </Text>
      </Section>
      <Section title="Alignment">
        <Text style={{ color: colors.text, textAlign: "left" }}>left ◀</Text>
        <Text style={{ color: colors.text, textAlign: "center" }}>▶ center ◀</Text>
        <Text style={{ color: colors.text, textAlign: "right" }}>▶ right</Text>
      </Section>
    </>
  );
}

// ─── Layout ─────────────────────────────────────────────────────────────

/** A box outlined in line characters, three rows tall, its label in the middle row. */
function Box(props: { label: string; flex?: number; width?: number }) {
  return (
    <View
      style={{
        flex: props.flex,
        width: props.width,
        height: 48,
        borderWidth: 1,
        borderColor: colors.line,
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Text style={{ color: colors.text, fontWeight: "700" }}>{props.label}</Text>
    </View>
  );
}

export function Layout() {
  const justify = signal<"flex-start" | "center" | "flex-end" | "space-between" | "space-around">("space-between");
  return (
    <>
      <Section title="flex: 1 · 2 · 1">
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Box label="1" flex={1} />
          <Box label="2" flex={2} />
          <Box label="1" flex={1} />
        </View>
      </Section>
      <Section title={`justifyContent: ${justify()}`}>
        <View style={{ flexDirection: "row", gap: 8, flexWrap: "wrap" }}>
          <Each of={["flex-start", "center", "flex-end", "space-between", "space-around"] as const}>
            {(j) => <Button label={j()} active={justify() === j()} onPress={() => justify.set(j())} />}
          </Each>
        </View>
        <View style={{ flexDirection: "row", justifyContent: justify() }}>
          <Box label="A" width={80} />
          <Box label="B" width={80} />
          <Box label="C" width={80} />
        </View>
      </Section>
      <Section title="Nesting and borders">
        <View style={{ flexDirection: "row", gap: 16 }}>
          <View style={{ flex: 1, borderWidth: 1, borderColor: colors.line, paddingHorizontal: 15, paddingVertical: 15 }}>
            <Text style={{ color: colors.text }}>Square corners</Text>
          </View>
          <View style={{ flex: 1, borderWidth: 1, borderRadius: 8, borderColor: colors.line, paddingHorizontal: 15, paddingVertical: 15 }}>
            <Text style={{ color: colors.text }}>Rounded corners</Text>
          </View>
          <View style={{ flex: 1, borderLeftWidth: 1, borderColor: colors.line, paddingLeft: 15 }}>
            <Text style={{ color: colors.text }}>A left rule</Text>
            <Text style={{ color: colors.muted }}>quote-like</Text>
          </View>
        </View>
      </Section>
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
        500 rows in one scroll view: wheel, ↑ ↓, PgUp / PgDn. Click a row to mark it ·{" "}
        <Text style={{ color: colors.accent }}>{done().size} marked</Text>
      </Text>
      <View>
        <Each of={items}>
          {(item) => {
            const it = item();
            const ticked = () => done().has(it.id);
            return (
              <View focusStyle={{ backgroundColor: colors.focus }} onPress={() => toggle(it.id)} style={{ flexDirection: "row", height: 16, gap: 16 }}>
                <Text style={{ width: 48, textAlign: "right", color: colors.faint }}>{String(it.id)}</Text>
                <Text style={{ color: ticked() ? colors.accent : colors.faint }}>{ticked() ? "*" : " "}</Text>
                <Text style={{ flex: 1, color: ticked() ? colors.accent : colors.text }}>{it.title}</Text>
                <Text style={{ width: 96, textAlign: "right", paddingRight: 8, color: colors.faint }}>{(it.size / 100).toFixed(1)} KB</Text>
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
      <View style={{ flexDirection: "row", gap: 16 }}>
        <Button label={running() ? "Pause" : "Live"} onPress={() => running.set(!running())} active={!running()} />
        <Text style={{ color: colors.muted }}>
          Drawn on a <Text style={{ color: colors.text }}>{"<Canvas>"}</Text> with paths: ratatui's canvas in braille dots, 2 × 4 to a cell
        </Text>
      </View>
      <Section title="oscilloscope · amplitude">
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
      </Section>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Section title="spectroscope · level" grow>
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
                cx.fillStyle = level > 0.6 ? colors.down : level > 0.3 ? colors.warn : colors.accent;
                cx.fillRect(i * 8 + 1, height - h, 4, h);
              }
              cx.fillStyle = colors.faint;
              cx.fillText("frequency →", width - 96, 12);
            }}
          />
        </Section>
        <VerticalRule />
        <Section title="vectorscope · left / right">
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
        </Section>
      </View>
    </>
  );
}
