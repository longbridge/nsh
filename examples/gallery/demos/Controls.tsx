import { type AnimatedValue, cleanup, computed, createAnimatedValue, effect, type JSX, Scroll, signal, Text, token, View } from "nsh";
import { type CalendarItemState, Checkbox, Collapsible, Combobox, DatePicker, type DateRangePreset, NumberInput, OtpInput, Progress, Radio, RadioGroup, Select, type SelectGroup, type SelectItemState, Slider, Switch, Tab, Tabs } from "nsh/base";
import { reduceMotion } from "nsh";
import { platform, windowSize } from "nsh";
import { Icon, type IconName } from "../icons";
import { alpha, Badge, Button, Caption, Card, colors, DemoPage, finish, focusRing, follow, hovered, IconTile, mono, motion, radius, Segmented, sliderFill, sliderLook, switchLook, tints, type Tone, toneColor } from "../ui";

// `nsh/base`'s controls own presses, keys, the disabled state and what
// screen readers say; every value is the app's, and so is the look: the
// gallery's own here, with its hover and keyboard focus ring (`hovered`,
// `focusRing`) on every control. So is the motion, gpui-kit's: marks and
// fills come and go with `motion.control`'s spring, thumbs and indicators
// travel with `motion.move`'s (`follow`), and land at once with reduced
// motion.

/**
 * The sizes of the page. Touch screens take the standard tier (44 pt and
 * more to press); a pointer the compact one desktops' settings and filters
 * use: smaller controls, rows and type, whatever the window's width.
 */
const size =
  platform.idiom === "desktop"
    ? { row: 40, padding: 6, gap: 10, label: 14, detail: 12, control: 16, corner: 5, check: 12, dot: 6, tabs: 30, line: 32, clear: 16, switch: "small" as const }
    : { row: 56, padding: 8, gap: 12, label: 15, detail: 13, control: 22, corner: 7, check: 16, dot: 8, tabs: 40, line: 44, clear: 20, switch: "regular" as const };
const pointer = platform.idiom === "desktop";

/** A control's row: the control and its label, the whole row pressable. */
const row = (first: boolean) => ({
  minHeight: size.row,
  flexDirection: "row" as const,
  alignItems: "center" as const,
  gap: size.gap,
  paddingVertical: size.padding,
  borderTopWidth: first ? 0 : 1,
  borderTopColor: colors.line,
});
/** How far a row's hover fill reaches past its content on each side, and its corners. */
const BLEED = 8;

/**
 * A row that takes the pointer's hover: `row`'s, its fill reaching `BLEED`
 * past the content on both sides with rounded corners, and the content where
 * it was (`start`: more room before it). The hairline above it is a
 * `Divider`, the length `row`'s top border had; the point it took stays in
 * the padding, so nothing moves.
 */
const hoverRow = (first: boolean, start = 0) => ({
  ...row(true),
  marginHorizontal: -BLEED,
  paddingStart: BLEED + start,
  paddingEnd: BLEED,
  paddingTop: size.padding + (first ? 0 : 1),
  borderRadius: BLEED,
});

/** The hairline above a `hoverRow` that is not the first. */
function Divider(props: { show: boolean }) {
  return (
    <If when={props.show}>
      <View style={{ position: "absolute", top: 0, left: BLEED, right: BLEED, height: 1, backgroundColor: colors.line }} />
    </If>
  );
}

const pressed = { opacity: 0.7 };
const dimmed = { opacity: 0.4 };

/** The fill of a box or ring that is off: the field's well. */
const empty = () => ({ backgroundColor: colors.field, backgroundImage: "none", borderColor: colors.fieldBorder, boxShadow: finish.field.boxShadow });

/** The fill of a box or ring that is on (the main button's citrus finish:
 * its darker olive edge and lit top), over the empty one and covering its
 * edge: it fades in and out as the value changes. Gradients do not
 * interpolate, so the finish fades whole rather than its colors. */
function Lit(props: { on: AnimatedValue; radius: number }) {
  return (
    <View
      style={{ pointerEvents: "none", position: "absolute", top: -1, left: -1, right: -1, bottom: -1, borderRadius: props.radius, borderWidth: 1, ...finish.light, opacity: props.on }}
    />
  );
}

/** A layer over the whole of its parent, its content centered: layers that cross over in place. */
const centered = { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, alignItems: "center", justifyContent: "center" } as const;

type Mark = "check" | "dash" | null;

/** A box's or ring's size: `scale` times the page's (0.8 small, 1.25 large). */
const dims = (scale = 1) => ({ control: Math.round(size.control * scale), check: Math.round(size.check * scale), corner: Math.round(size.corner * scale), dot: Math.round(size.dot * scale) });

/** A checkbox's box: its frame, empty; `BoxMark` fills it. `round`: a circle (a task's, as reminders draw it). */
const box = (scale = 1, round = false) => {
  const d = dims(scale);
  return {
    width: d.control,
    height: d.control,
    borderRadius: round ? d.control / 2 : d.corner,
    alignItems: "center" as const,
    justifyContent: "center" as const,
    borderWidth: 1,
    ...empty(),
  };
};

/**
 * The press feedback of a box or a ring: it sinks a little while its row is
 * pressed (the row is the `group`), and springs back.
 */
const sink = "transition-transform duration-150 ease-out group-active:scale-90 motion-reduce:transition-none";

/** What a checkbox's box shows: citrus with a check or a dash, drawn with the same bold pen (about 2 pt at the desktop size). */
const markWeight = 3.2;
function BoxMark(props: { mark: Mark; scale?: number; round?: boolean }) {
  const d = dims(props.scale);
  // Each part follows the value on a spring of its own and stays mounted
  // while it fades out (unmounting it on the value showed only the fade
  // in): a check turning into a dash crosses over on a fill that stays.
  const on = follow(() => (props.mark ? 1 : 0));
  const check = follow(() => (props.mark === "check" ? 1 : 0));
  const dash = follow(() => (props.mark === "dash" ? 1 : 0));
  /** A mark: centered in the box, growing a little as it fades in. */
  const glyph = (t: AnimatedValue, name: "check" | "minus") => (
    <View
      style={{
        ...centered,
        pointerEvents: "none",
        opacity: t,
        transform: [{ scale: t.interpolate({ inputRange: [0, 1], outputRange: [0.75, 1] }) }],
      }}
    >
      <Icon name={name} size={d.check} tint={colors.onAccent} weight={markWeight} />
    </View>
  );
  // One view as large as the box inside its border holds the layers: the
  // slot's content sits in a holder view sized to it (docs/dev/components.md
  // "Children, slots and rows"), so layers placed only absolutely made it
  // 0 × 0 and the fill a speck at the box's middle.
  return (
    <View style={{ width: d.control - 2, height: d.control - 2 }}>
      <Lit on={on} radius={props.round ? d.control / 2 : d.corner} />
      {glyph(check, "check")}
      {glyph(dash, "minus")}
    </View>
  );
}

/** A radio's ring: empty, or citrus with a dot that grows in as it is checked. Its row is the `group` it sinks with. */
function Ring(props: { on: boolean; scale?: number }) {
  const d = dims(props.scale);
  const on = follow(() => (props.on ? 1 : 0));
  return (
    <View
      class={sink}
      style={{ width: d.control, height: d.control, borderRadius: d.control / 2, alignItems: "center", justifyContent: "center", borderWidth: 1, ...empty() }}
    >
      <Lit on={on} radius={d.control / 2} />
      <View
        style={{
          width: d.dot,
          height: d.dot,
          borderRadius: d.dot / 2,
          backgroundColor: colors.onAccent,
          opacity: on,
          transform: [{ scale: on.interpolate({ inputRange: [0, 1], outputRange: [0.5, 1] }) }],
        }}
      />
    </View>
  );
}

/**
 * An animated value that runs from 0 to 1 on `motion.control`'s spring each
 * time `key` changes: content swapped in (a tab's, an opened section's)
 * settles into place rather than appearing. At rest at once with reduced
 * motion.
 */
function settleOn(key: () => unknown): AnimatedValue {
  const t = createAnimatedValue(1);
  let shown = key();
  effect(() => {
    const next = key();
    if (next === shown) return;
    shown = next;
    if (reduceMotion()) return;
    t.set(0);
    t.spring(1, motion.control);
  });
  return t;
}

/**
 * A scale at rest at 1 that bounces a little past it each time `key`
 * changes (`motion.move`'s spring with a kick, about 7% at its peak), so
 * what changed pops; still with reduced motion.
 */
function popOn(key: () => unknown): AnimatedValue {
  const s = createAnimatedValue(1);
  let shown = key();
  effect(() => {
    const next = key();
    if (next === shown) return;
    shown = next;
    if (!reduceMotion()) s.spring(1, { ...motion.move, velocity: 4 });
  });
  return s;
}

/** Content that settles in with `t` (`settleOn`): it fades in and drops a few points into place. */
const settle = (t: AnimatedValue) => ({ opacity: t, transform: [{ translateY: t.interpolate({ inputRange: [0, 1], outputRange: [-6, 0] }) }] });

/**
 * A choice card (a checkbox's or a radio's): raised while off; edged in the
 * accent over its tint while on, the change faded. It sinks a little while
 * pressed (`activeStyle`), at once with reduced motion.
 */
const tile = (on: boolean) => ({
  flex: 1,
  minWidth: 0,
  padding: 12,
  paddingTop: 14,
  borderRadius: radius.inset,
  borderWidth: 1,
  borderColor: on ? colors.accent : colors.border,
  backgroundColor: on ? tints.accent : colors.raised,
  boxShadow: on ? "none" : finish.raised.boxShadow,
  transitionProperty: "transform, border-color, background-color, box-shadow",
  transitionDuration: reduceMotion() ? 0 : 150,
});

/** A determinate bar's fill: the whole track's width, slid back by what is
 * left to do (`remaining`) so a new value glides there, on gpui-kit's 180 ms
 * `easing_move` curve (and a new tone fades in); the track clips it. */
const glide = "w-full transition duration-180 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none";
/** The fill's `translate` for `value` percent. */
const remaining = (value: number) => `${Math.max(0, Math.min(100, value)) - 100}% 0`;
/** A bar's rail, `height` points tall: the slider's. */
const rail = (height = 6) => ({ height, borderRadius: height / 2, overflow: "hidden" as const, backgroundColor: colors.track, boxShadow: `inset 0 0 0 1px ${finish.track.borderColor}, ${finish.track.boxShadow}` });
/** A bar's fill: the slider's citrus run, or a tone's color. */
const fillOf = (height = 6, tone?: Tone) => (tone ? { height, borderRadius: height / 2, backgroundColor: toneColor(tone) } : { height, borderRadius: height / 2, ...sliderFill(0, 1) });

/**
 * Where the selected item of a row (tabs, segments) lies, for an indicator
 * that travels to it as gpui-kit's tab bar's does (`motion.move`): each item
 * reports its place (`measure`, from its `onLayout`), and `left` and `width`
 * spring to the selected one's, from the row's outer edge. They are placed
 * at once on the first layout, as the row is resized and with reduced
 * motion.
 */
function indicator(index: () => number) {
  const spans = signal<{ x: number; width: number }[]>([]);
  const span = () => spans()[index()];
  const left = createAnimatedValue(0);
  const width = createAnimatedValue(0);
  let at = -1;
  effect(() => {
    const i = index();
    const s = span();
    if (!s) return;
    const jump = at === -1 || at === i || reduceMotion();
    at = i;
    if (jump) {
      left.set(s.x);
      width.set(s.width);
    } else {
      left.spring(s.x, motion.move);
      width.spring(s.width, motion.move);
    }
  });
  return {
    span,
    left,
    width,
    measure(i: number, x: number, w: number) {
      const next = spans().slice();
      next[i] = { x, width: w };
      spans.set(next);
    },
  };
}

/** The room a tab title's icon takes: the icon and the gap after it. */
const ICON_ROOM = 14 + 6;

/**
 * A tab's label (and icon): the text color while selected, fading in as the
 * indicator arrives. It keeps its natural width (never truncated) and
 * reports it without the icon (`onWidth`): the same whether the icon shows
 * or not, so a row deciding on its icons by it never flips back.
 */
function TabTitle(props: { name: string; selected: boolean; count?: number; icon?: IconName; onWidth?: (bare: number) => void }) {
  const on = follow(() => (props.selected ? 1 : 0), motion.move);
  const away = on.interpolate({ inputRange: [0, 1], outputRange: [1, 0] });
  return (
    <View
      onLayout={({ bounds }) => props.onWidth?.(bounds.size.width - (props.icon ? ICON_ROOM : 0))}
      style={{ flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}
    >
      <If when={props.icon}>
        {/* The icon's two tints cross over, as the label's color fades. */}
        <View style={{ width: 14, height: 14 }}>
          <View style={{ position: "absolute", opacity: away }}>
            <Icon name={props.icon!} size={14} tint={colors.muted} />
          </View>
          <View style={{ position: "absolute", opacity: on }}>
            <Icon name={props.icon!} size={14} tint={colors.text} />
          </View>
        </View>
      </If>
      <Text style={{ numberOfLines: 1, fontSize: 13, fontWeight: "500", color: on.interpolate({ inputRange: [0, 1], outputRange: [colors.muted, colors.text] }) }}>
        {props.name}
      </Text>
      <If when={props.count !== undefined}>
        <View style={{ minWidth: 18, height: 16, paddingHorizontal: 5, borderRadius: 8, alignItems: "center", justifyContent: "center", backgroundColor: colors.inset, borderWidth: 1, borderColor: colors.line }}>
          <Text style={{ fontSize: 10, fontWeight: "600", color: colors.muted, fontVariant: ["tabular-nums"] }}>{props.count}</Text>
        </View>
      </If>
    </View>
  );
}

/**
 * The widths of a row of tab titles without their icons (`TabTitle`'s
 * `onWidth`, `report(i)`), and the row's width (`room`, from its
 * `onLayout`): what a row decides on its icons by. `widths()` is empty until
 * every title has reported.
 */
function titles(count: number) {
  const bare = signal<number[]>([]);
  const room = signal(0);
  return {
    room,
    widths: () => (bare().filter((w) => w !== undefined).length === count ? bare() : []),
    report: (i: number) => (w: number) => {
      if (Math.abs((bare()[i] ?? -1) - w) < 0.5) return;
      const next = bare().slice();
      next[i] = w;
      bare.set(next);
    },
  };
}

/** A label, and a quieter line under it. */
function Label(props: { label: string; detail?: string }) {
  return (
    <View style={{ flex: 1, gap: 2 }}>
      <Text style={{ fontSize: size.label, color: colors.text }}>{props.label}</Text>
      <If when={props.detail}>
        <Text style={{ fontSize: size.detail, color: colors.muted }}>{props.detail}</Text>
      </If>
    </View>
  );
}

const KINDS: { name: string; count: number }[] = [
  { name: "Photos", count: 1204 },
  { name: "Videos", count: 86 },
  { name: "Screenshots", count: 312 },
  { name: "Live Photos", count: 48 },
];

const PLANS: { name: string; detail: string; price: string; disabled?: boolean }[] = [
  { name: "Free", detail: "One device, 5 GB", price: "$0" },
  { name: "Plus", detail: "Three devices, 200 GB", price: "$3 / mo" },
  { name: "Team", detail: "A shared library, 2 TB", price: "$12 / mo" },
  { name: "Enterprise", detail: "Set up by your organization", price: "—", disabled: true },
];

const TASKS = ["Freeze the API", "Write the release notes", "Record the compatibility baseline", "Tag the release", "Announce it"];

const COOKIES: { name: string; detail: string; required?: boolean }[] = [
  { name: "Necessary", detail: "Signing in, security and your settings", required: true },
  { name: "Analytics", detail: "Which pages are visited, counted without who visited them" },
  { name: "Personalization", detail: "Remembers your region and the layout you chose" },
  { name: "Marketing", detail: "Shows our offers on other sites" },
];

const CHANNELS: { name: string; detail: string; icon: "bell" | "chat" | "document"; disabled?: boolean }[] = [
  { name: "Push", detail: "As it happens", icon: "bell" },
  { name: "Email", detail: "A daily digest", icon: "document" },
  { name: "SMS", detail: "Add a number first", icon: "chat", disabled: true },
];

const SIZES = [
  { name: "Small", scale: 0.8 },
  { name: "Regular", scale: 1 },
  { name: "Large", scale: 1.25 },
];

const SPEEDS: { name: string; detail: string; price: string; disabled?: boolean }[] = [
  { name: "Standard", detail: "In 3 to 5 days", price: "Included" },
  { name: "Express", detail: "Tomorrow", price: "$9" },
  { name: "Same day", detail: "Not in your area", price: "$19", disabled: true },
];

const TEXT_SIZES: { name: string; scale: number; text: number; disabled?: boolean }[] = [
  { name: "Small", scale: 0.8, text: 13 },
  { name: "Medium", scale: 1, text: 15 },
  { name: "Large", scale: 1.25, text: 18 },
  { name: "Huge", scale: 1.25, text: 22, disabled: true },
];

const SECTIONS: { name: string; icon: IconName; rows: [string, string][]; disabled?: boolean }[] = [
  {
    name: "Overview",
    icon: "chart",
    rows: [
      ["Visitors", "12,480"],
      ["Sign-ups", "342"],
      ["Conversion", "2.74%"],
    ],
  },
  {
    name: "Activity",
    icon: "bell",
    rows: [
      ["Deployed to production", "09:41"],
      ["Invoice paid", "08:15"],
      ["A member joined", "07:02"],
    ],
  },
  {
    name: "Members",
    icon: "chat",
    rows: [
      ["Ada Lovelace", "Owner"],
      ["Grace Hopper", "Admin"],
      ["Alan Turing", "Member"],
    ],
  },
  { name: "Billing", icon: "document", rows: [], disabled: true },
];

const PROJECT: { name: string; icon: "code" | "spark" | "diff" | "document"; count?: number; rows: [string, string][]; disabled?: boolean }[] = [
  {
    name: "Code",
    icon: "code",
    rows: [
      ["crates/", "2 hours ago"],
      ["examples/", "yesterday"],
      ["README.md", "last week"],
    ],
  },
  {
    name: "Issues",
    icon: "spark",
    count: 12,
    rows: [
      ["Text clips when the window shrinks", "#482"],
      ["A slower first frame on Android 12", "#476"],
      ["Support right-to-left menus", "#451"],
    ],
  },
  {
    name: "Pull requests",
    icon: "diff",
    count: 3,
    rows: [
      ["Animate the controls' state changes", "#601"],
      ["Sort the gallery's components", "#600"],
      ["Read fonts from the bundle", "#597"],
    ],
  },
  { name: "Wiki", icon: "document", rows: [], disabled: true },
];

const FAQ = [
  { q: "Can I change my plan later?", a: "Yes. A new plan starts at once, and what is left of the old one is credited to your next invoice." },
  { q: "What happens to my photos if I cancel?", a: "They stay yours: you can download them for 30 days, then the library is deleted for good." },
  { q: "Do you offer a discount for teams?", a: "Teams of ten or more pay 20% less per seat, billed once a year." },
];

const USAGE: [string, string][] = [
  ["Photos", "120 GB"],
  ["Apps", "54 GB"],
  ["System", "38 GB"],
];

const FILES: { name: string; size: string; icon: "photos" | "media" | "document"; pace: number }[] = [
  { name: "Lisbon, 48 photos", size: "212 MB", icon: "photos", pace: 9 },
  { name: "Interview.mov", size: "1.2 GB", icon: "media", pace: 4 },
  { name: "Contract.pdf", size: "2 MB", icon: "document", pace: 25 },
];

const STEPS = ["Account", "Profile", "Team", "Review"];

const METERS: { name: string; height: number; note: (v: number) => string }[] = [
  { name: "Battery", height: 4, note: (v) => (v < 20 ? `${v}%, charge soon` : `${v}%`) },
  { name: "Storage", height: 6, note: (v) => (v > 85 ? `${v}% used, almost full` : `${v}% used`) },
  { name: "Profile", height: 10, note: (v) => `${v}% complete` },
];

/** Listings per $25 of price, from $0 to $500. */
const HISTOGRAM = [3, 6, 11, 18, 26, 31, 34, 29, 24, 22, 19, 15, 12, 10, 8, 6, 5, 3, 2, 1];
const HISTOGRAM_MAX = Math.max(...HISTOGRAM);
/** Whether the histogram's bar `i` (its prices from `i` × 25 up to the next bar's) holds a price in the range, both ends
 * included: a range closed to one price lights the bar that price falls in. */
const inRange = (i: number, [from, to]: [number, number]) => (i + 1) * 25 > from && i * 25 <= to;
/** How many listings the price range holds (the bars it covers). */
const listings = (range: [number, number]) => HISTOGRAM.reduce((n, c, i) => (inRange(i, range) ? n + c : n), 0);

const BANDS = ["60", "250", "1k", "4k", "16k"];
const EQ_PRESETS = [
  { value: "Flat" as const, label: "Flat" },
  { value: "Bass" as const, label: "Bass" },
  { value: "Vocal" as const, label: "Vocal" },
];
const PRESETS = { Flat: [0, 0, 0, 0, 0], Bass: [8, 5, 0, -2, -1], Vocal: [-3, 0, 5, 4, 1] };

const count = (n: number) => n.toLocaleString("en-US");

const SORTS = [
  { value: "newest", title: "Newest first" },
  { value: "oldest", title: "Oldest first" },
  { value: "name", title: "Name" },
  { value: "size", title: "Size" },
  { value: "kind", title: "Kind" },
];

const FONTS = [
  { value: "helvetica", title: "Helvetica", group: "Sans serif" },
  { value: "inter", title: "Inter", group: "Sans serif" },
  { value: "roboto", title: "Roboto", group: "Sans serif" },
  { value: "source-sans", title: "Source Sans", group: "Sans serif" },
  { value: "georgia", title: "Georgia", group: "Serif" },
  { value: "merriweather", title: "Merriweather", group: "Serif" },
  { value: "playfair", title: "Playfair Display", group: "Serif", disabled: true },
  { value: "times", title: "Times New Roman", group: "Serif" },
  { value: "fira-code", title: "Fira Code", group: "Monospace" },
  { value: "jetbrains-mono", title: "JetBrains Mono", group: "Monospace" },
  { value: "menlo", title: "Menlo", group: "Monospace" },
  { value: "sf-mono", title: "SF Mono", group: "Monospace" },
];

const LANGUAGES = [
  { value: "en", title: "English" },
  { value: "de", title: "Deutsch" },
];

const TOPICS = [
  { value: "layout", title: "Layout", group: "Core" },
  { value: "text", title: "Text", group: "Core" },
  { value: "animation", title: "Animation", group: "Core" },
  { value: "ios", title: "iOS", group: "Platforms" },
  { value: "android", title: "Android", group: "Platforms" },
  { value: "desktop", title: "Desktop", group: "Platforms" },
  { value: "web", title: "Web", group: "Platforms" },
  { value: "legacy", title: "Legacy", group: "Platforms", disabled: true },
];

const REVIEWERS = [
  { value: "ada", title: "Ada" },
  { value: "grace", title: "Grace" },
  { value: "linus", title: "Linus" },
];

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS = ["S", "M", "T", "W", "T", "F", "S"];

/** A day's ISO date (`"2026-10-28"`), in local time as the calendar's days are. */
const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const addDays = (d: Date, days: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + days);
/** An ISO date's month and day as a trigger shows them: `Oct 28`. */
const monthDay = (value: string) => `${MONTHS[Number(value.slice(5, 7)) - 1].slice(0, 3)} ${Number(value.slice(8, 10))}`;
/** An ISO date (and time) as a trigger shows it: `Oct 28, 2026`, `Oct 28, 2026 · 09:30`. */
const shortDate = (value: string) => `${monthDay(value)}, ${value.slice(0, 4)}${value.length > 10 ? ` · ${value.slice(11, 16)}` : ""}`;

const titleOf = (items: { value: string; title: string }[], value: string | null) => items.find((i) => i.value === value)?.title ?? "None";

/** The controls, one demo each in the sidebar. */
export type Control =
  | "Switch"
  | "Checkbox"
  | "Radio"
  | "Tabs"
  | "Select"
  | "Combobox"
  | "DatePicker"
  | "Collapsible"
  | "Progress"
  | "Slider"
  | "NumberInput"
  | "OtpInput";

/**
 * One of `nsh/base`'s controls (`Switch`, `Checkbox`, `Radio` and
 * `RadioGroup`, `Tab` and `Tabs`, `Select`, `Combobox`, `DatePicker`, `Collapsible`, `Progress`, `Slider`) in
 * the settings, filters, pickers, section switchers, choices and disclosures apps use
 * it for, then in other looks, sizes and states (choice cards, tiles,
 * underlined tabs, an accordion), above the last event it reported.
 */
export function Controls(props: { show: Control }) {
  const last = signal("—");

  function Settings() {
    const notifications = signal(true);
    const badges = signal(true);
    const previews = signal(false);
    const haptics = signal(false);
    const lowData = signal(false);
    function Setting(props: { label: string; detail: string; icon: IconName; on: boolean; set: (on: boolean) => void; disabled?: boolean; first?: boolean }) {
      const look = switchLook(() => props.on, size.switch);
      return (
        <Switch
          checked={props.on}
          disabled={props.disabled}
          onChange={({ checked: on }) => {
            props.set(on);
            last.set(`${props.label}: ${on ? "on" : "off"}`);
          }}
          // The track at the trailing edge, after the label; the icon leads.
          style={{ ...hoverRow(!!props.first), flexDirection: "row-reverse" }}
          activeStyle={pressed}
          hoverStyle={props.disabled ? undefined : hovered()}
          focusStyle={focusRing()}
          disabledStyle={dimmed}
          trackStyle={look.trackStyle}
          thumbStyle={look.thumbStyle}
        >
          <Divider show={!props.first} />
          <Label label={props.label} detail={props.detail} />
          <IconTile name={props.icon} size={pointer ? 26 : 32} tint={props.on ? colors.accent : colors.muted} />
        </Switch>
      );
    }
    /** A group's heading in the settings list. */
    function Group(props: { title: string; first?: boolean }) {
      return <Text style={{ marginTop: props.first ? 0 : size.gap, marginBottom: 2, fontSize: 11, fontWeight: "600", letterSpacing: 0.6, color: colors.faint }}>{props.title.toUpperCase()}</Text>;
    }
    const off = "Turn on notifications first";
    return (
      <Card icon="controls" title="Switch" subtitle="A settings page: two groups, two settings that need another on">
        <View>
          <Group first title="Alerts" />
          <Setting
            first
            label="Notifications"
            icon="bell"
            detail={notifications() ? "Banners, sounds and badges" : "Off"}
            on={notifications()}
            set={(on) => notifications.set(on)}
          />
          <Setting
            label="Badges"
            icon="spark"
            detail={notifications() ? "The unread count on the icon" : off}
            on={badges()}
            disabled={!notifications()}
            set={(on) => badges.set(on)}
          />
          <Setting
            label="Previews"
            icon="chat"
            detail={notifications() ? (previews() ? "The message's text in banners" : "Only who wrote") : off}
            on={previews()}
            disabled={!notifications()}
            set={(on) => previews.set(on)}
          />
          <Group title="Device" />
          <Setting first label="Haptics" icon="gestures" detail="A tap under the finger as values change" on={haptics()} set={(on) => haptics.set(on)} />
          <Setting label="Low data mode" icon="storage" detail={lowData() ? "Images load at a lower quality" : "Images load in full"} on={lowData()} set={(on) => lowData.set(on)} />
        </View>
      </Card>
    );
  }

  function Quick() {
    const on = signal<string[]>(["Sounds", "Night Shift"]);
    const has = (name: string) => on().includes(name);
    // Focus silences the sounds: turning it on turns them off and holds them.
    const held = (name: string) => name === "Sounds" && has("Focus");
    /** A quick setting: a tile whose icon lights up and pops as it changes, its switch in the corner. */
    function QuickTile(props: { name: string; icon: "moon" | "sun" | "bell" | "spark"; detail: string }) {
      const t = follow(() => (has(props.name) ? 1 : 0));
      const pop = popOn(() => has(props.name));
      const look = switchLook(() => has(props.name), "small");
      const away = t.interpolate({ inputRange: [0, 1], outputRange: [1, 0] });
      return (
        <Switch
          checked={has(props.name)}
          disabled={held(props.name)}
          onChange={({ checked }) => {
            let next = checked ? [...on(), props.name] : on().filter((n) => n !== props.name);
            if (props.name === "Focus" && checked) next = next.filter((n) => n !== "Sounds");
            on.set(next);
            last.set(`${props.name}: ${checked ? "on" : "off"}`);
          }}
          style={{ ...tile(has(props.name)), width: "48%", flex: undefined, gap: 2 }}
          activeStyle={{ transform: [{ scale: 0.97 }] }}
          hoverStyle={held(props.name) || has(props.name) ? undefined : { borderColor: colors.faint }}
          focusStyle={focusRing()}
          disabledStyle={dimmed}
          trackStyle={{ ...look.trackStyle, position: "absolute", top: 12, right: 12 }}
          thumbStyle={look.thumbStyle}
        >
          <View style={{ width: 32, height: 32, marginBottom: 8, borderRadius: 16, alignItems: "center", justifyContent: "center", borderWidth: 1, ...empty(), transform: [{ scale: pop }] }}>
            <Lit on={t} radius={16} />
            <View style={{ ...centered, opacity: away }}>
              <Icon name={props.icon} size={16} tint={colors.muted} />
            </View>
            <View style={{ ...centered, opacity: t }}>
              <Icon name={props.icon} size={16} tint={colors.onAccent} />
            </View>
          </View>
          <Text style={{ fontSize: size.label, fontWeight: "600", color: colors.text }}>{props.name}</Text>
          <Text style={{ fontSize: size.detail, color: colors.muted }}>{held(props.name) ? "Silenced by Focus" : has(props.name) ? props.detail : "Off"}</Text>
        </Switch>
      );
    }
    return (
      <Card title="Quick settings" subtitle="Tiles: the icon lights up with the switch; Focus silences the sounds">
        <View style={{ flexDirection: "row", flexWrap: "wrap", justifyContent: "space-between", rowGap: 8 }}>
          <QuickTile name="Focus" icon="moon" detail="Until tomorrow" />
          <QuickTile name="Night Shift" icon="sun" detail="Until 7:00" />
          <QuickTile name="Sounds" icon="bell" detail="Ring and alerts" />
          <QuickTile name="Low power" icon="spark" detail="Background work paused" />
        </View>
      </Card>
    );
  }

  function Filter() {
    const shown = signal<string[]>(["Photos", "Videos", "Live Photos"]);
    const has = (name: string) => shown().includes(name);
    const total = computed(() => KINDS.filter((k) => has(k.name)).reduce((n, k) => n + k.count, 0));
    const all = () => shown().length === KINDS.length;
    const mark = (): Mark => (all() ? "check" : shown().length > 0 ? "dash" : null);
    return (
      <Card icon="controls" title="Checkbox" subtitle="A filter: “Select all” shows a partial selection" right={<Badge label={`${shown().length} of ${KINDS.length}`} />}>
        <View>
          <Checkbox
            checked={all()}
            indeterminate={mark() === "dash"}
            onChange={({ checked: on }) => {
              shown.set(on ? KINDS.map((k) => k.name) : []);
              last.set(`Select all: ${on}`);
            }}
            class="group"
            style={hoverRow(true)}
            activeStyle={pressed}
            hoverStyle={hovered()}
            focusStyle={focusRing()}
            indicatorClass={sink}
            indicatorStyle={box()}
            indicator={<BoxMark mark={mark()} />}
          >
            <Text style={{ flex: 1, fontSize: size.label, fontWeight: "600", color: colors.text }}>Select all</Text>
          </Checkbox>
          <Each of={KINDS}>
            {(kind) => (
              <Checkbox
                checked={has(kind().name)}
                onChange={({ checked: on }) => {
                  shown.set(on ? KINDS.map((k) => k.name).filter((n) => n === kind().name || has(n)) : shown().filter((n) => n !== kind().name));
                  last.set(`${kind().name}: ${on}`);
                }}
                // One level under "Select all": the boxes line up with its label.
                class="group"
                style={hoverRow(false, size.control + size.gap)}
                activeStyle={pressed}
                hoverStyle={hovered()}
                focusStyle={focusRing()}
                indicatorClass={sink}
                indicatorStyle={box()}
                indicator={<BoxMark mark={has(kind().name) ? "check" : null} />}
              >
                <Divider show />
                <Text style={{ flex: 1, fontSize: size.label, color: colors.text }}>{kind().name}</Text>
                <Text style={{ fontSize: size.detail, color: colors.faint, fontVariant: ["tabular-nums"] }}>{count(kind().count)}</Text>
              </Checkbox>
            )}
          </Each>
        </View>
        <Caption label={shown().length ? `Showing ${count(total())} items` : "Nothing to show"} />
      </Card>
    );
  }

  function Checklist() {
    const done = signal<string[]>(["Freeze the API", "Write the release notes"]);
    const has = (task: string) => done().includes(task);
    const left = () => TASKS.length - done().length;
    /** A task: a round box, its label fading back and struck through once done. */
    function Task(props: { task: string; first: boolean }) {
      const on = follow(() => (has(props.task) ? 1 : 0));
      return (
        <Checkbox
          checked={has(props.task)}
          onChange={({ checked: on }) => {
            done.set(on ? [...done(), props.task] : done().filter((t) => t !== props.task));
            last.set(`${props.task}: ${on ? "done" : "to do"}`);
          }}
          class="group"
          style={hoverRow(props.first)}
          activeStyle={pressed}
          hoverStyle={hovered()}
          focusStyle={focusRing()}
          indicatorClass={sink}
          indicatorStyle={box(1, true)}
          indicator={<BoxMark mark={has(props.task) ? "check" : null} round />}
        >
          <Divider show={!props.first} />
          <Text
            style={{
              flex: 1,
              fontSize: size.label,
              color: on.interpolate({ inputRange: [0, 1], outputRange: [colors.text, colors.faint] }),
              textDecorationLine: has(props.task) ? "line-through" : "none",
            }}
          >
            {props.task}
          </Text>
        </Checkbox>
      );
    }
    return (
      <Card title="Checklist" subtitle="Round boxes: a task done fades back, and the bar fills" right={<Badge label={`${done().length} of ${TASKS.length}`} tone={left() === 0 ? "up" : "neutral"} />}>
        <View>
          <Each of={TASKS}>{(task, i) => <Task task={task()} first={i() === 0} />}</Each>
        </View>
        <Progress value={(done().length / TASKS.length) * 100} accessibilityLabel="Release readiness" trackStyle={rail(4)} indicatorClass={glide} indicatorStyle={{ ...fillOf(4), translate: remaining((done().length / TASKS.length) * 100) }} />
        <Caption label={left() === 0 ? "Ready to ship" : `${left()} to go before the release`} />
      </Card>
    );
  }

  function Consent() {
    const allowed = signal<string[]>(["Analytics"]);
    const has = (name: string) => allowed().includes(name);
    const saved = signal(false);
    const set = (next: string[]) => {
      allowed.set(next);
      saved.set(false);
    };
    return (
      <Card title="Consent" subtitle="Choices with a description each; the necessary one cannot be turned off">
        <View>
          <Each of={COOKIES}>
            {(c, i) => (
              <Checkbox
                checked={c().required || has(c().name)}
                disabled={c().required}
                onChange={({ checked }) => {
                  set(checked ? [...allowed(), c().name] : allowed().filter((n) => n !== c().name));
                  last.set(`${c().name}: ${checked}`);
                }}
                class="group"
                // The box at the first line of the label, not its middle.
                style={{ ...hoverRow(i() === 0), alignItems: "flex-start" }}
                activeStyle={pressed}
                hoverStyle={c().required ? undefined : hovered()}
                focusStyle={focusRing()}
                disabledStyle={{ opacity: 0.6 }}
                indicatorClass={sink}
                indicatorStyle={{ ...box(), marginTop: 2 }}
                indicator={<BoxMark mark={c().required || has(c().name) ? "check" : null} />}
              >
                <Divider show={i() !== 0} />
                <Label label={c().required ? `${c().name} (always on)` : c().name} detail={c().detail} />
              </Checkbox>
            )}
          </Each>
        </View>
        <View style={{ flexDirection: "row", gap: size.gap, alignSelf: pointer ? "flex-end" : "stretch" }}>
          <Button
            label="Accept all"
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              set(COOKIES.filter((c) => !c.required).map((c) => c.name));
              last.set("Consent: all accepted");
            }}
          />
          <Button
            label={saved() ? "Saved" : "Save choices"}
            size={size.switch}
            flex={pointer ? undefined : 1}
            disabled={saved()}
            onPress={() => {
              saved.set(true);
              last.set(`Consent: ${allowed().join(", ") || "necessary only"}`);
            }}
          />
        </View>
      </Card>
    );
  }

  function Channels() {
    const on = signal<string[]>(["Push"]);
    const has = (name: string) => on().includes(name);
    return (
      <Card title="Choice cards" subtitle="The whole card is the checkbox: its edge and fill follow the value">
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Each of={CHANNELS}>
            {(c) => (
              <Checkbox
                checked={has(c().name)}
                disabled={c().disabled}
                onChange={({ checked }) => {
                  on.set(checked ? [...on(), c().name] : on().filter((n) => n !== c().name));
                  last.set(`${c().name}: ${checked}`);
                }}
                class="group"
                style={tile(has(c().name))}
                activeStyle={{ transform: [{ scale: 0.97 }] }}
                hoverStyle={c().disabled || has(c().name) ? undefined : { borderColor: colors.faint }}
                focusStyle={focusRing()}
                disabledStyle={dimmed}
                // The box in the card's corner.
                indicatorClass={sink}
                indicatorStyle={{ ...box(0.8), position: "absolute", top: 10, right: 10 }}
                indicator={<BoxMark mark={has(c().name) ? "check" : null} scale={0.8} />}
              >
                <Icon name={c().icon} size={18} tint={has(c().name) ? colors.accent : colors.muted} />
                <Text style={{ marginTop: 6, fontSize: size.label, fontWeight: "600", color: colors.text }}>{c().name}</Text>
                <Text style={{ fontSize: size.detail, color: colors.muted }}>{c().detail}</Text>
              </Checkbox>
            )}
          </Each>
        </View>
        <Caption label={on().length ? `Notifying you by ${on().join(" and ").toLowerCase()}` : "No notifications"} />
      </Card>
    );
  }

  function BoxStates() {
    const sizes = signal<string[]>(["Regular"]);
    const has = (name: string) => sizes().includes(name);
    /** A checkbox and its label on one line, `scale` times the page's size. */
    function Inline(props: { label: string; mark: Mark; scale?: number; disabled?: boolean; onChange?: (checked: boolean) => void }) {
      return (
        <Checkbox
          checked={props.mark === "check"}
          indeterminate={props.mark === "dash"}
          disabled={props.disabled}
          onChange={({ checked }) => props.onChange?.(checked)}
          class="group"
          style={{ flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 4 }}
          activeStyle={pressed}
          focusStyle={focusRing(2)}
          disabledStyle={dimmed}
          indicatorClass={sink}
          indicatorStyle={box(props.scale)}
          indicator={<BoxMark mark={props.mark} scale={props.scale} />}
        >
          <Text style={{ fontSize: size.label, color: colors.text }}>{props.label}</Text>
        </Checkbox>
      );
    }
    const line = { flexDirection: "row" as const, flexWrap: "wrap" as const, alignItems: "center" as const, columnGap: 20, rowGap: 6 };
    return (
      <Card title="Sizes and states" subtitle="Three sizes; disabled while unchecked, checked and indeterminate">
        <View style={line}>
          <Each of={SIZES}>
            {(z) => (
              <Inline
                label={z().name}
                scale={z().scale}
                mark={has(z().name) ? "check" : null}
                onChange={(checked) => {
                  sizes.set(checked ? [...sizes(), z().name] : sizes().filter((n) => n !== z().name));
                  last.set(`${z().name}: ${checked}`);
                }}
              />
            )}
          </Each>
        </View>
        <View style={{ height: 1, backgroundColor: colors.line }} />
        <View style={line}>
          <Inline label="Unavailable" mark={null} disabled />
          <Inline label="Required" mark="check" disabled />
          <Inline label="Partly set" mark="dash" disabled />
        </View>
        <Caption label={pointer ? "Hover and Tab to each: the disabled ones take neither" : "The disabled ones take no press"} />
      </Card>
    );
  }

  function Plans() {
    const plan = signal("Plus");
    const chosen = () => PLANS.find((p) => p.name === plan())!;
    return (
      <Card icon="controls" title="Radio" subtitle="A plan picker: one of a small set, all in view">
        <RadioGroup>
          <Each of={PLANS}>
            {(p, i) => (
              <Radio
                checked={plan() === p().name}
                disabled={p().disabled}
                onChange={() => {
                  plan.set(p().name);
                  last.set(`Plan: ${p().name}`);
                }}
                class="group"
                style={hoverRow(i() === 0)}
                activeStyle={pressed}
                hoverStyle={p().disabled ? undefined : hovered()}
                focusStyle={focusRing()}
                disabledStyle={dimmed}
              >
                <Divider show={i() !== 0} />
                <Ring on={plan() === p().name} />
                <Label label={p().name} detail={p().detail} />
                <Text style={{ fontSize: size.label, color: plan() === p().name ? colors.text : colors.muted, fontVariant: ["tabular-nums"] }}>{p().price}</Text>
              </Radio>
            )}
          </Each>
        </RadioGroup>
        {/* The commit of the picker: under it on a phone, at its trailing edge with a pointer. */}
        <View style={{ alignItems: pointer ? "flex-end" : "stretch" }}>
          <Button
            label={`Continue with ${plan()}`}
            size={pointer ? "small" : "regular"}
            onPress={() => last.set(`Continue: ${chosen().name}, ${chosen().price}`)}
          />
        </View>
      </Card>
    );
  }

  function Delivery() {
    const speed = signal("Standard");
    return (
      <Card title="Choice cards" subtitle="Delivery: the whole card is the radio, the ring in its corner">
        <RadioGroup style={{ flexDirection: "row", gap: 8 }}>
          <Each of={SPEEDS}>
            {(d) => (
              <Radio
                checked={speed() === d().name}
                disabled={d().disabled}
                onChange={() => {
                  speed.set(d().name);
                  last.set(`Delivery: ${d().name}`);
                }}
                class="group"
                style={tile(speed() === d().name)}
                activeStyle={{ transform: [{ scale: 0.97 }] }}
                hoverStyle={d().disabled || speed() === d().name ? undefined : { borderColor: colors.faint }}
                focusStyle={focusRing()}
                disabledStyle={dimmed}
              >
                <View style={{ position: "absolute", top: 10, right: 10 }}>
                  <Ring on={speed() === d().name} scale={0.8} />
                </View>
                <Text style={{ marginRight: 20, fontSize: size.label, fontWeight: "600", color: colors.text }}>{d().name}</Text>
                <Text style={{ fontSize: size.detail, color: colors.muted }}>{d().detail}</Text>
                <Text style={{ marginTop: 8, fontSize: size.label, color: speed() === d().name ? colors.accent : colors.text, fontVariant: ["tabular-nums"] }}>{d().price}</Text>
              </Radio>
            )}
          </Each>
        </RadioGroup>
      </Card>
    );
  }

  function Reading() {
    const choice = signal("Medium");
    const chosen = () => TEXT_SIZES.find((t) => t.name === choice())!;
    const preview = settleOn(choice);
    return (
      <Card title="Sizes" subtitle="Rings in three sizes, one locked; the preview follows the choice">
        <RadioGroup style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "center", columnGap: 20, rowGap: 6 }}>
          <Each of={TEXT_SIZES}>
            {(t) => (
              <Radio
                checked={choice() === t().name}
                disabled={t().disabled}
                onChange={() => {
                  choice.set(t().name);
                  last.set(`Text size: ${t().name}`);
                }}
                class="group"
                style={{ flexDirection: "row", alignItems: "center", gap: 8, paddingVertical: 4 }}
                activeStyle={pressed}
                focusStyle={focusRing(2)}
                disabledStyle={dimmed}
              >
                <Ring on={choice() === t().name} scale={t().scale} />
                <Text style={{ fontSize: size.label, color: colors.text }}>{t().name}</Text>
              </Radio>
            )}
          </Each>
        </RadioGroup>
        <View style={{ padding: 12, borderRadius: radius.inset, backgroundColor: colors.inset, borderWidth: 1, borderColor: colors.line, ...finish.inset }}>
          <View style={settle(preview)}>
            <Text style={{ fontSize: chosen().text, lineHeight: Math.round(chosen().text * 1.45), color: colors.text }}>
              Native views, styled by the app: every state and every motion is the gallery's own.
            </Text>
          </View>
        </View>
      </Card>
    );
  }

  function Sections() {
    const section = signal("Overview");
    const shown = () => SECTIONS.find((s) => s.name === section())!;
    // The track's padding and border: a tab is a pill inside them.
    const inset = 3;
    const inner = size.tabs - 2 * (inset + 1);
    // The pill under the selected tab travels to the next one; the tabs
    // share the track equally, so only its offset moves (placed inside the
    // track's border).
    const pill = indicator(() => SECTIONS.findIndex((s) => s.name === section()));
    const rows = settleOn(section);
    // The tabs share the track equally: icons show while every title fits
    // its share with its icon (a tab's border and padding are 14).
    const sized = titles(SECTIONS.length);
    const icons = () => {
      const widths = sized.widths();
      const share = (sized.room() - 2 * (inset + 1) - 2 * (SECTIONS.length - 1)) / SECTIONS.length;
      return widths.length > 0 && widths.every((w) => w + ICON_ROOM + 14 <= share);
    };
    return (
      <Card icon="controls" title="Tabs" subtitle="A section switcher: the app shows the selected one">
        <Tabs
          onLayout={({ bounds }) => sized.room.set(bounds.size.width)}
          style={{
            height: size.tabs,
            flexDirection: "row",
            gap: 2,
            padding: inset,
            borderRadius: size.tabs / 2,
            borderWidth: 1,
            borderColor: colors.border,
            backgroundColor: colors.inset,
            boxShadow: finish.track.boxShadow,
          }}
        >
          <View
            style={{
              pointerEvents: "none",
              position: "absolute",
              top: inset,
              bottom: inset,
              left: 0,
              width: pill.span()?.width ?? 0,
              opacity: pill.span() ? 1 : 0,
              borderRadius: inner / 2,
              borderWidth: 1,
              ...finish.pill,
              transform: [{ translateX: pill.left.sub(1) }],
            }}
          />
          <Each of={SECTIONS}>
            {(s, i) => (
              <Tab
                selected={section() === s().name}
                disabled={s().disabled}
                onPress={() => {
                  section.set(s().name);
                  last.set(`Tab: ${s().name}`);
                }}
                onLayout={({ bounds }) => pill.measure(i(), bounds.origin.x, bounds.size.width)}
                style={{ flex: 1, paddingHorizontal: 6, borderRadius: inner / 2, borderWidth: 1, borderColor: "transparent", alignItems: "center", justifyContent: "center" }}
                hoverStyle={s().disabled || section() === s().name ? undefined : hovered()}
                focusStyle={focusRing()}
                disabledStyle={dimmed}
              >
                <TabTitle name={s().name} icon={icons() ? s().icon : undefined} onWidth={sized.report(i())} selected={section() === s().name} />
              </Tab>
            )}
          </Each>
        </Tabs>
        <View style={settle(rows)}>
          <Each of={shown().rows}>
            {(r, i) => (
              <View style={{ ...row(i() === 0), minHeight: size.line, paddingVertical: 0 }}>
                <Text style={{ flex: 1, fontSize: size.label, color: colors.text }}>{r()[0]}</Text>
                <Text style={{ fontSize: size.label, color: colors.muted, fontVariant: ["tabular-nums"] }}>{r()[1]}</Text>
              </View>
            )}
          </Each>
        </View>
      </Card>
    );
  }

  function Project() {
    const tab = signal("Issues");
    const shown = () => PROJECT.find((t) => t.name === tab())!;
    const line = indicator(() => PROJECT.findIndex((t) => t.name === tab()));
    const rows = settleOn(tab);
    // The underline is a bar `BASE` wide, moved to the selected tab's middle
    // and stretched to its width: its edges travel together, as both values
    // spring alike.
    const BASE = 100;
    // Icons show while the whole strip fits the card with them (a tab's
    // border and padding are 18, 2 between tabs); without them it scrolls
    // when it still does not fit.
    const sized = titles(PROJECT.length);
    const icons = () => {
      const widths = sized.widths();
      return widths.length > 0 && widths.reduce((sum, w) => sum + w + ICON_ROOM + 18, 2 * (PROJECT.length - 1)) <= sized.room();
    };
    return (
      <Card title="Underlined" subtitle="Tabs as wide as their labels: the line travels and stretches to the next">
        {/* The strip scrolls when the labels do not fit; the hairline under it is outside the scroll view, so the
            strip's clipping never cuts it, and the underline (at the strip's bottom) covers it. */}
        <View onLayout={({ bounds }) => sized.room.set(bounds.size.width)} style={{ marginHorizontal: -8 }}>
          <View style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 1, backgroundColor: colors.line }} />
          <Scroll horizontal scrollbar={false} style={{ flexGrow: 0 }}>
            <Tabs style={{ flexDirection: "row", gap: 2, paddingTop: 3 }}>
              <View
                style={{
                  pointerEvents: "none",
                  position: "absolute",
                  left: 0,
                  bottom: 0,
                  width: BASE,
                  height: 2,
                  borderRadius: 1,
                  backgroundColor: colors.accent,
                  opacity: line.span() ? 1 : 0,
                  transform: [{ translateX: line.left.add(line.width.mul(0.5)).sub(BASE / 2) }, { scaleX: line.width.mul(1 / BASE) }],
                }}
              />
              <Each of={PROJECT}>
                {(t, i) => (
                  <Tab
                    selected={tab() === t().name}
                    disabled={t().disabled}
                    onPress={() => {
                      tab.set(t().name);
                      last.set(`Project: ${t().name}`);
                    }}
                    onLayout={({ bounds }) => line.measure(i(), bounds.origin.x, bounds.size.width)}
                    style={{ height: size.tabs, paddingHorizontal: 8, marginBottom: 4, borderRadius: radius.control, borderWidth: 1, borderColor: "transparent", justifyContent: "center" }}
                    hoverStyle={t().disabled || tab() === t().name ? undefined : hovered()}
                    activeStyle={pressed}
                    focusStyle={focusRing()}
                    disabledStyle={dimmed}
                  >
                    <TabTitle name={t().name} icon={icons() ? t().icon : undefined} onWidth={sized.report(i())} count={t().count} selected={tab() === t().name} />
                  </Tab>
                )}
              </Each>
            </Tabs>
          </Scroll>
        </View>
        <View style={settle(rows)}>
          <Each of={shown().rows}>
            {(r, i) => (
              <View style={{ ...row(i() === 0), minHeight: size.line, paddingVertical: 0 }}>
                <Icon name={shown().icon} size={size.check} tint={colors.faint} />
                <Text style={{ flex: 1, numberOfLines: 1, fontSize: size.label, color: colors.text }}>{r()[0]}</Text>
                <Text style={{ fontSize: size.detail, color: colors.faint, fontVariant: ["tabular-nums"] }}>{r()[1]}</Text>
              </View>
            )}
          </Each>
        </View>
      </Card>
    );
  }

  // The lists' look, shared by the select's and the combobox's cards.
  /** The list: a popup at the trigger with a pointer, as the menus'; a sheet at the bottom on a touch screen. */
  const surface = () =>
    pointer
      ? { backgroundColor: colors.raised, borderRadius: radius.inset, borderWidth: 1, borderColor: colors.border, padding: 6, ...finish.raised }
      : { backgroundColor: colors.surface, borderTopLeftRadius: 20, borderTopRightRadius: 20, paddingHorizontal: 10, paddingTop: 10, paddingBottom: 34, maxHeight: "70%" as const };
  const backdrop = { backgroundColor: "rgba(0,0,0,0.45)" };
  /** The clear button's inset from the trigger's end: centered where the chevron's lane is. The button is in the
      select's view, outside the trigger, so its inset counts the trigger's 1 pt border too. */
  const clearInset = 1 + 10 + size.check / 4 - size.clear / 2;
  /** The trigger: the chosen title and a chevron that turns while the list is open, its border lit then. */
  function Picker(props: { title: string; open: boolean; disabled?: boolean; placeholder?: boolean; clearable?: boolean }) {
    return (
      <View
        style={{
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          height: size.line,
          paddingLeft: 10,
          // Room for the clear button, which goes over the trigger's end in place of the chevron.
          paddingRight: props.clearable ? clearInset + size.clear + 8 : 10,
          borderRadius: radius.control,
          borderWidth: 1,
          borderColor: props.open ? colors.faint : colors.border,
          backgroundColor: colors.raised,
          ...finish.raised,
          // Dimmed while the select is: the app knows, as it passes `disabled`.
          opacity: props.disabled ? dimmed.opacity : 1,
        }}
        hoverStyle={props.disabled ? undefined : { borderColor: colors.faint }}
        activeStyle={pressed}
        focusStyle={focusRing()}
        disabledStyle={dimmed}
      >
        <Text style={{ numberOfLines: 1, fontSize: size.label, color: props.placeholder ? colors.faint : colors.text }}>{props.title}</Text>
        {/* The chevron's icon box, taking only the room of its stroke (half the box) in the row, so it turns
            about its own center; it points down, and up while open. */}
        <View
          class="transition-transform duration-150 motion-reduce:transition-none"
          style={{ display: props.clearable ? "none" : "flex", marginHorizontal: -size.check / 4, transform: [{ rotate: props.open ? "-90deg" : "90deg" }] }}
        >
          <Icon name="chevron" size={size.check} tint={colors.muted} />
        </View>
      </View>
    );
  }
  /** A row of the list: lit while highlighted (the keys or the pointer), bolder with a check while chosen. */
  function Option(props: { item: SelectItemState; note?: string }) {
    return (
      <View
        style={{
          flexDirection: "row",
          alignItems: "center",
          gap: 8,
          height: pointer ? 32 : 46,
          paddingHorizontal: 8,
          borderRadius: 7,
          borderWidth: 1,
          borderColor: props.item.highlighted ? alpha(colors.accent, 0.3) : "transparent",
          backgroundColor: props.item.highlighted ? tints.accent : "transparent",
          opacity: props.item.disabled ? 0.35 : 1,
        }}
      >
        <Text style={{ numberOfLines: 1, flex: 1, fontSize: 14, fontWeight: props.item.selected ? "600" : "400", color: colors.text }}>
          {props.item.title}
        </Text>
        <If when={props.note}>
          <Text style={{ fontSize: 12, color: colors.faint }}>{props.note}</Text>
        </If>
        <View style={{ width: 16, alignItems: "center" }}>
          <If when={props.item.selected}>
            <Icon name="check" size={14} tint={colors.accent} />
          </If>
        </View>
      </View>
    );
  }
  function Header(props: { group: SelectGroup }) {
    return (
      <Text style={{ fontSize: 11, fontWeight: "600", letterSpacing: 0.6, color: colors.faint, paddingHorizontal: 9, paddingTop: 8, paddingBottom: 4 }}>
        {props.group.title.toUpperCase()}
      </Text>
    );
  }
  const opened = (set: (open: boolean) => void) => ({ open }: { open: boolean }) => set(open);

  function Choices() {
    const sort = signal<string | null>("newest");
    const font = signal<string | null>("inter");
    const sortOpen = signal(false);
    const fontOpen = signal(false);
    return (
      <Card icon="controls" title="Select" subtitle="A choice from a list: the app owns the value">
        <View>
          <View style={row(true)}>
            <Label label="Sort by" detail={pointer ? "Press a letter to jump to an order" : "Five orders in a sheet"} />
            <Select
              items={SORTS}
              value={sort()}
              onChange={({ value: next }) => {
                sort.set(next);
                last.set(`Sort by: ${titleOf(SORTS, next)}`);
              }}
              onOpenChange={opened((open) => sortOpen.set(open))}
              accessibilityLabel="Sort by"
              offset={4}
              margin={8}
              align="end"
              // Wider than the trigger with a pointer: the titles whole.
              popupStyle={pointer ? { ...surface(), width: 200 } : surface()}
              backdropStyle={backdrop}
              itemClass="active:opacity-70"
              trigger={<Picker title={titleOf(SORTS, sort())} open={sortOpen()} />}
              renderItem={(item) => <Option item={item} />}
            />
          </View>
          <View style={row(false)}>
            <Label label="Font" detail="Search, sections, one not installed" />
            <Select
              items={FONTS}
              value={font()}
              onChange={({ value: next }) => {
                font.set(next);
                last.set(next === null ? "Font: cleared" : `Font: ${titleOf(FONTS, next)}`);
              }}
              onOpenChange={opened((open) => fontOpen.set(open))}
              accessibilityLabel="Font"
              searchable
              searchPlaceholder="Search fonts"
              searchClass={`${pointer ? "text-sm" : "text-base"} text-foreground placeholder:text-faint`}
              searchStyle={{
                height: pointer ? 32 : 40,
                paddingHorizontal: 9,
                marginBottom: 6,
                borderRadius: radius.control,
                borderWidth: 1,
                borderColor: colors.fieldBorder,
                backgroundColor: colors.field,
                ...finish.field,
              }}
              empty={<Text style={{ paddingVertical: 20, textAlign: "center", fontSize: 14, color: colors.muted }}>No font matches</Text>}
              offset={4}
              margin={8}
              align="end"
              popupStyle={pointer ? { ...surface(), width: 260 } : surface()}
              backdropStyle={backdrop}
              itemClass="active:opacity-70"
              cleanable
              clearButtonAccessibilityLabel="Clear the font"
              clearButtonClass="items-center justify-center rounded-full enabled:hover:bg-accent active:bg-track"
              clearButtonStyle={{ position: "absolute", top: (size.line - size.clear) / 2, right: clearInset, width: size.clear, height: size.clear }}
              clearButton={<Icon name="close" size={size.clear - 6} tint={colors.muted} />}
              trigger={<Picker title={font() === null ? "Default" : titleOf(FONTS, font())} open={fontOpen()} placeholder={font() === null} clearable={font() !== null} />}
              renderItem={(item) => <Option item={item} note={item.disabled ? "Not installed" : undefined} />}
              renderGroup={(group) => <Header group={group} />}
            />
          </View>
          <View style={row(false)}>
            <Label label="Language" detail="Set by your organization" />
            <Select items={LANGUAGES} value="en" disabled accessibilityLabel="Language" trigger={<Picker title="English" open={false} disabled />} />
          </View>
        </View>
      </Card>
    );
  }

  function Tags() {
    const topics = signal<string[]>(["layout", "ios"]);
    const reviewer = signal<string[]>([]);
    const topicsOpen = signal(false);
    const reviewerOpen = signal(false);
    const names = (items: { value: string; title: string }[], values: string[]) => values.map((v) => titleOf(items, v));
    const topicsTitle = () => {
      const all = names(TOPICS, topics());
      return all.length <= 2 ? all.join(", ") : `${all.length} topics`;
    };
    return (
      <Card icon="controls" title="Combobox" subtitle="Several values from a list: the app owns them">
        <View>
          <View style={row(true)}>
            <Label label="Topics" detail="Several at once, with search; the list stays open" />
            <Combobox
              items={TOPICS}
              multiple
              value={topics()}
              onChange={({ value }) => {
                topics.set(value);
                last.set(`Topics: ${names(TOPICS, value).join(", ") || "none"}`);
              }}
              onConfirm={({ value }) => last.set(`Topics confirmed: ${value.length}`)}
              onOpenChange={opened((open) => topicsOpen.set(open))}
              accessibilityLabel="Topics"
              searchable
              searchPlaceholder="Search topics"
              searchClass={`${pointer ? "text-sm" : "text-base"} text-foreground placeholder:text-faint`}
              searchStyle={{
                height: pointer ? 32 : 40,
                paddingHorizontal: 9,
                marginBottom: 6,
                borderRadius: radius.control,
                borderWidth: 1,
                borderColor: colors.fieldBorder,
                backgroundColor: colors.field,
                ...finish.field,
              }}
              empty={<Text style={{ paddingVertical: 20, textAlign: "center", fontSize: 14, color: colors.muted }}>No topic matches</Text>}
              offset={4}
              margin={8}
              align="end"
              popupStyle={pointer ? { ...surface(), width: 240 } : surface()}
              backdropStyle={backdrop}
              itemClass="active:opacity-70"
              cleanable
              clearButtonAccessibilityLabel="Clear the topics"
              clearButtonClass="items-center justify-center rounded-full enabled:hover:bg-accent active:bg-track"
              clearButtonStyle={{ position: "absolute", top: (size.line - size.clear) / 2, right: clearInset, width: size.clear, height: size.clear }}
              clearButton={<Icon name="close" size={size.clear - 6} tint={colors.muted} />}
              footerStyle={{ marginTop: 6, paddingTop: 6, paddingHorizontal: 9, borderTopWidth: 1, borderColor: colors.border }}
              footer={<Text style={{ fontSize: 12, color: colors.muted }}>{`${topics().length} selected`}</Text>}
              trigger={<Picker title={topics().length ? topicsTitle() : "Add topics"} open={topicsOpen()} placeholder={!topics().length} clearable={topics().length > 0} />}
              renderItem={(item) => <Option item={item} note={item.disabled ? "Archived" : undefined} />}
              renderGroup={(group) => <Header group={group} />}
            />
          </View>
          <View style={row(false)}>
            <Label label="Reviewer" detail="One person: a choice replaces the last" />
            <Combobox
              items={REVIEWERS}
              value={reviewer()}
              onChange={({ value }) => {
                reviewer.set(value);
                last.set(`Reviewer: ${names(REVIEWERS, value)[0] ?? "none"}`);
              }}
              onOpenChange={opened((open) => reviewerOpen.set(open))}
              accessibilityLabel="Reviewer"
              offset={4}
              margin={8}
              align="end"
              popupStyle={pointer ? { ...surface(), width: 200 } : surface()}
              backdropStyle={backdrop}
              itemClass="active:opacity-70"
              trigger={<Picker title={names(REVIEWERS, reviewer())[0] ?? "Nobody"} open={reviewerOpen()} placeholder={!reviewer().length} />}
              renderItem={(item) => <Option item={item} />}
            />
          </View>
        </View>
      </Card>
    );
  }

  // The calendar's look, the date picker's card's.
  /** A day's cell: seven across the popup with a pointer, across a phone's sheet otherwise. */
  const cell = pointer ? 30 : 42;
  /** The gap between the months of a calendar of two. */
  const monthGap = 16;
  /**
   * An item of the calendar (`renderItem`): Previous and Next as chevrons, the month and year toggles
   * (or each month's title) by name, the weekdays' initials, and the days: today on the theme's quiet `accent`
   * fill (gpui-component's), the selected ones on the accent's, the days of a complete range on a band from end to end (`range` says where it
   * starts and ends), the other months' days and the disabled ones faint.
   */
  function CalendarItem(props: { item: CalendarItemState; range?: [string | null, string | null] | null }) {
    const kind = props.item.kind;
    if (kind === "previous" || kind === "next") {
      return (
        <View style={{ width: cell, height: cell, alignItems: "center", justifyContent: "center", opacity: props.item.disabled ? 0.3 : 1 }}>
          <View style={{ transform: [{ rotate: kind === "previous" ? "180deg" : "0deg" }] }}>
            <Icon name="chevron" size={size.check} tint={colors.muted} />
          </View>
        </View>
      );
    }
    if (kind === "monthToggle" || kind === "yearToggle") {
      // A month's title in a calendar of several takes no press: the month and its year, over the month.
      const title = kind === "monthToggle" && props.item.disabled;
      return (
        <Text
          style={{
            width: title ? 6 * cell + monthGap / 2 : kind === "monthToggle" ? 3 * cell : 2 * cell,
            paddingHorizontal: 3,
            textAlign: title ? "center" : kind === "monthToggle" ? "right" : "left",
            fontSize: size.label,
            fontWeight: "600",
            color: props.item.active ? colors.accent : colors.text,
          }}
        >
          {title ? `${MONTHS[props.item.value - 1]} ${props.item.year}` : kind === "monthToggle" ? MONTHS[props.item.value - 1] : String(props.item.value)}
        </Text>
      );
    }
    if (kind === "weekday") {
      return <Text style={{ width: cell, paddingVertical: 4, textAlign: "center", fontSize: 11, fontWeight: "600", color: colors.faint }}>{WEEKDAYS[props.item.value]}</Text>;
    }
    if (kind === "month" || kind === "year") {
      // The month view's months (rows of 3) and the year view's years (rows of 5), as wide as a month.
      return (
        <View
          style={{
            width: (7 * cell) / (kind === "month" ? 3 : 5),
            height: cell + 6,
            alignItems: "center",
            justifyContent: "center",
            borderRadius: radius.control,
            backgroundColor: props.item.active ? colors.accentFill : "transparent",
          }}
        >
          <Text style={{ fontSize: size.label, color: props.item.active ? colors.onAccent : colors.text }}>
            {kind === "month" ? MONTHS[props.item.value - 1].slice(0, 3) : String(props.item.value)}
          </Text>
        </View>
      );
    }
    // A day. The band reaches from the start's middle to the end's.
    const band = () => {
      const r = props.range;
      if (!props.item.inRange || !r || r[0] === r[1]) return null;
      return { left: props.item.date === r[0] ? cell / 2 : 0, right: props.item.date === r[1] ? cell / 2 : 0 };
    };
    return (
      <View style={{ width: cell, height: cell, alignItems: "center", justifyContent: "center" }}>
        <View style={{ position: "absolute", top: 2, bottom: 2, left: band()?.left ?? 0, right: band()?.right ?? 0, backgroundColor: band() ? tints.accent : "transparent" }} />
        <View
          style={{
            width: cell - 4,
            height: cell - 4,
            borderRadius: (cell - 4) / 2,
            alignItems: "center",
            justifyContent: "center",
            borderWidth: 1.5,
            // The keyboard's day ringed.
            borderColor: props.item.focused ? colors.accent : "transparent",
            backgroundColor: props.item.active ? colors.accentFill : props.item.today ? token("accent") : "transparent",
          }}
        >
          <Text
            style={{
              fontSize: pointer ? 13 : 15,
              fontVariant: ["tabular-nums"],
              fontWeight: props.item.active || props.item.today ? "600" : "400",
              color: props.item.active ? colors.onAccent : props.item.muted ? colors.faint : colors.text,
              opacity: props.item.disabled ? 0.5 : 1,
            }}
          >
            {String(props.item.value)}
          </Text>
        </View>
      </View>
    );
  }
  /** The time field's segments (`renderSegment`): the selected one filled while the field has the keys. */
  function Segment(props: { label: string; selected: boolean }) {
    return (
      <Text
        style={{
          paddingHorizontal: 3,
          borderRadius: 4,
          fontSize: size.label,
          fontVariant: ["tabular-nums"],
          color: props.selected ? colors.onAccent : colors.text,
          backgroundColor: props.selected ? colors.accentFill : "transparent",
        }}
      >
        {props.label}
      </Text>
    );
  }
  /** The popup's layout: the presets beside the calendar with a pointer, above it on a phone. */
  const content = pointer ? { flexDirection: "row" as const, gap: 10 } : { gap: 12 };
  const presets = pointer
    ? { gap: 2, minWidth: 110, paddingRight: 10, borderRightWidth: 1, borderColor: colors.border }
    : { flexDirection: "row" as const, flexWrap: "wrap" as const, gap: 6 };
  const preset = pointer
    ? { height: 28, paddingHorizontal: 8, borderRadius: radius.control, justifyContent: "center" as const, fontSize: 13, color: colors.text }
    : { height: 34, paddingHorizontal: 12, borderRadius: 17, borderWidth: 1, borderColor: colors.border, justifyContent: "center" as const, fontSize: 14, color: colors.text };
  const header = { marginBottom: 4 };
  const calendar = pointer ? { padding: 4 } : { alignSelf: "center" as const };
  /** The time's row under a hairline, its label at the start and the field at the end, on the calendar's edges. */
  const time = { gap: 10, marginTop: 8, paddingTop: 8, borderTopWidth: 1, borderColor: colors.border, ...(pointer ? { paddingHorizontal: 4 } : { alignSelf: "center" as const, width: 7 * cell }) };
  /** The time's field: a recessed well as the number inputs', with their hover and the keyboard's ring while it edits. */
  const timeField = [
    pointer ? "h-8 rounded-md px-2" : "h-11 rounded-lg px-3",
    "items-center border border-input bg-field",
    "enabled:hover:border-faint focus-within:outline-2 focus-within:outline-info",
  ].join(" ");

  function Dates() {
    const due = signal<string | null>(null);
    const trip = signal<[string | null, string | null] | null>(null);
    const meeting = signal<string | null>(null);
    const dueOpen = signal(false);
    const tripOpen = signal(false);
    const meetingOpen = signal(false);
    // Two months side by side when the window has room for them and the presets.
    const twoMonths = () => pointer && windowSize().width >= 600;
    // The presets from today: the coming weekend, the next week and month.
    const today = new Date();
    const saturday = addDays(today, (6 - today.getDay() + 7) % 7);
    const trips: DateRangePreset[] = [
      { label: "Weekend", value: [iso(saturday), iso(addDays(saturday, 1))] },
      { label: "Next 7 days", value: [iso(today), iso(addDays(today, 6))] },
      { label: "Next 30 days", value: [iso(today), iso(addDays(today, 29))] },
    ];
    const tripTitle = () => {
      const r = trip();
      return r && r[0] && r[1] ? `${monthDay(r[0])} – ${monthDay(r[1])}` : "Pick dates";
    };
    return (
      <Card icon="controls" title="DatePicker" subtitle="A date, a range or a date and time: the app owns the value">
        <View>
          <View style={row(true)}>
            <Label label="Due date" detail="Weekdays only; clear it to have none" />
            <DatePicker
              value={due()}
              onChange={({ value }) => {
                const next = typeof value === "string" ? value : null;
                due.set(next);
                last.set(`Due: ${next ?? "none"}`);
              }}
              onOpenChange={opened((open) => dueOpen.set(open))}
              accessibilityLabel="Due date"
              disabledMatcher={[0, 6]}
              offset={4}
              margin={8}
              align="end"
              popupStyle={surface()}
              backdropStyle={backdrop}
              calendarStyle={calendar}
              headerStyle={header}
              itemClass="active:opacity-70"
              renderItem={(item) => <CalendarItem item={item} />}
              cleanable
              clearButtonAccessibilityLabel="Clear the due date"
              clearButtonClass="items-center justify-center rounded-full enabled:hover:bg-accent active:bg-track"
              clearButtonStyle={{ position: "absolute", top: (size.line - size.clear) / 2, right: clearInset, width: size.clear, height: size.clear }}
              clearButton={<Icon name="close" size={size.clear - 6} tint={colors.muted} />}
              trigger={<Picker title={due() ? shortDate(due()!) : "No due date"} open={dueOpen()} placeholder={!due()} clearable={due() !== null} />}
            />
          </View>
          <View style={row(false)}>
            <Label label="Trip" detail={twoMonths() ? "Two months; a press starts the range, the next ends it" : "A press starts the range, the next ends it"} />
            <DatePicker
              range
              value={trip()}
              onChange={({ value }) => {
                const next = Array.isArray(value) ? value : null;
                trip.set(next);
                last.set(next ? `Trip: ${next[0]} → ${next[1]}` : "Trip: none");
              }}
              onOpenChange={opened((open) => tripOpen.set(open))}
              accessibilityLabel="Trip"
              numberOfMonths={twoMonths() ? 2 : 1}
              presets={trips}
              offset={4}
              margin={8}
              align="end"
              popupStyle={surface()}
              backdropStyle={backdrop}
              contentStyle={content}
              presetsStyle={presets}
              presetStyle={preset}
              presetClass="enabled:hover:bg-accent active:opacity-70"
              calendarStyle={calendar}
              headerStyle={header}
              bodyStyle={{ gap: monthGap }}
              itemClass="active:opacity-70"
              renderItem={(item) => <CalendarItem item={item} range={trip()} />}
              trigger={<Picker title={tripTitle()} open={tripOpen()} placeholder={!trip()} />}
            />
          </View>
          <View style={row(false)}>
            <Label label="Meeting" detail="A day, then its time; the day again closes" />
            <DatePicker
              value={meeting()}
              onChange={({ value }) => {
                const next = typeof value === "string" ? value : null;
                meeting.set(next);
                last.set(`Meeting: ${next ?? "none"}`);
              }}
              onOpenChange={opened((open) => meetingOpen.set(open))}
              accessibilityLabel="Meeting"
              timePrecision="minute"
              hourCycle="h23"
              defaultTime="09:00"
              offset={4}
              margin={8}
              align="end"
              popupStyle={surface()}
              backdropStyle={backdrop}
              calendarStyle={calendar}
              headerStyle={header}
              itemClass="active:opacity-70"
              renderItem={(item) => <CalendarItem item={item} />}
              timeStyle={time}
              timeLabel={<Text style={{ fontSize: size.label, color: colors.muted }}>Time</Text>}
              timeAccessibilityLabel="Meeting time"
              timeFieldClass={timeField}
              segmentClass="active:opacity-70"
              separatorStyle={{ color: colors.faint, fontSize: size.label }}
              renderSegment={(segment) => <Segment label={segment.label} selected={segment.selected} />}
              trigger={<Picker title={meeting() ? shortDate(meeting()!) : "Not scheduled"} open={meetingOpen()} placeholder={!meeting()} />}
            />
          </View>
        </View>
      </Card>
    );
  }

  /** The always-visible row that flips a section: the app's trigger, which tells screen readers whether it is expanded. */
  function Summary(props: { label: string; detail?: string; open: boolean; set: (open: boolean) => void; first?: boolean }) {
    return (
      <View
        role="button"
        accessibilityState={{ expanded: props.open }}
        onPress={() => {
          const next = !props.open;
          props.set(next);
          last.set(`${props.label}: ${next ? "expanded" : "collapsed"}`);
        }}
        style={hoverRow(!!props.first)}
        activeStyle={pressed}
        hoverStyle={hovered()}
        focusStyle={focusRing()}
      >
        <Divider show={!props.first} />
        <Label label={props.label} detail={props.detail} />
        {/* A lane as wide as the chevron's stroke (half its icon box), so the chevron ends where the values below it end. */}
        <View
          class="transition-transform duration-150 motion-reduce:transition-none"
          style={{ width: size.check / 2, alignItems: "center", rotate: props.open ? "90deg" : "0deg" }}
        >
          <Icon name="chevron" size={size.check} tint={colors.muted} />
        </View>
      </View>
    );
  }
  function Disclosure() {
    const storage = signal(false);
    const advanced = signal(false);
    const originals = signal(false);
    const look = switchLook(() => originals(), size.switch);
    // The content settles in as a section opens (the collapsible adds it
    // then, and takes it out at once as it closes).
    const storageIn = follow(() => (storage() ? 1 : 0));
    const advancedIn = follow(() => (advanced() ? 1 : 0));
    return (
      <Card icon="controls" title="Collapsible" subtitle="Details under a summary, kept while collapsed">
        <View>
          <Collapsible
            open={storage()}
            content={
              <View style={{ ...settle(storageIn), paddingStart: size.gap, paddingBottom: size.padding }}>
                <Each of={USAGE}>
                  {(u) => (
                    <View style={{ flexDirection: "row", alignItems: "center", minHeight: size.line }}>
                      <Text style={{ flex: 1, fontSize: size.label, color: colors.muted }}>{u()[0]}</Text>
                      <Text style={{ fontSize: size.label, color: colors.muted, fontVariant: ["tabular-nums"] }}>{u()[1]}</Text>
                    </View>
                  )}
                </Each>
              </View>
            }
          >
            <Summary first label="Storage" detail="212 GB of 256 GB used" open={storage()} set={(open) => storage.set(open)} />
          </Collapsible>
          <Collapsible
            open={advanced()}
            content={
              <View style={settle(advancedIn)}>
                <Switch
                  checked={originals()}
                  onChange={({ checked: on }) => {
                    originals.set(on);
                    last.set(`Keep originals: ${on ? "on" : "off"}`);
                  }}
                  style={{ ...hoverRow(false, size.gap), flexDirection: "row-reverse" }}
                  activeStyle={pressed}
                  hoverStyle={hovered()}
                  focusStyle={focusRing()}
                  trackStyle={look.trackStyle}
                  thumbStyle={look.thumbStyle}
                >
                  <Divider show />
                  <Label label="Keep originals" detail={originals() ? "Edits are saved as copies" : "Edits replace the photo"} />
                </Switch>
              </View>
            }
          >
            <Summary label="Advanced" detail="Options most people leave alone" open={advanced()} set={(open) => advanced.set(open)} />
          </Collapsible>
        </View>
      </Card>
    );
  }

  function Faq() {
    // One answer open at a time, as an accordion: opening one closes the last.
    const open = signal<string | null>(FAQ[0].q);
    function Item(props: { q: string; a: string; first: boolean }) {
      const t = follow(() => (open() === props.q ? 1 : 0));
      return (
        <Collapsible
          open={open() === props.q}
          content={
            <View style={{ ...settle(t), paddingBottom: size.padding + 4, paddingEnd: 24 }}>
              <Text style={{ fontSize: size.label, lineHeight: Math.round(size.label * 1.5), color: colors.muted }}>{props.a}</Text>
            </View>
          }
        >
          <Summary first={props.first} label={props.q} detail="" open={open() === props.q} set={(next) => open.set(next ? props.q : null)} />
        </Collapsible>
      );
    }
    return (
      <Card title="Accordion" subtitle="Questions: one answer open at a time">
        <View>
          <Each of={FAQ}>{(f, i) => <Item q={f().q} a={f().a} first={i() === 0} />}</Each>
        </View>
      </Card>
    );
  }

  function Transfers() {
    const sent = signal(0);
    const running = signal(false);
    let timer: ReturnType<typeof setInterval> | undefined;
    const stop = () => {
      if (timer !== undefined) clearInterval(timer);
      timer = undefined;
      running.set(false);
    };
    cleanup(stop);
    // The indeterminate bar's sweep: the app flips its translation, a
    // transition moves it; it rests in the middle with reduced motion.
    const end = signal(false);
    const sweep = setInterval(() => {
      if (!reduceMotion()) end.set(!end());
    }, 800);
    cleanup(() => clearInterval(sweep));
    const start = () => {
      if (sent() >= 100) sent.set(0);
      running.set(true);
      last.set("Upload: started");
      timer = setInterval(() => {
        sent.set(Math.min(100, sent() + 10));
        if (sent() >= 100) {
          stop();
          last.set("Upload: done");
        }
      }, 300);
    };
    /** A transfer: its label and value over a bar, the value tabular at the trailing edge. */
    function Transfer(props: { label: string; detail: string; value: number; indeterminate?: boolean; first?: boolean }) {
      return (
        <Progress
          value={props.value}
          indeterminate={props.indeterminate}
          // The label row first: the children follow the track.
          style={{ ...row(!!props.first), flexDirection: "column-reverse", alignItems: "stretch", gap: size.padding }}
          // The slider's rail and fill, so a level and a transfer read alike.
          trackStyle={rail()}
          // Determinate, the fill glides to each new value (`glide`).
          indicatorClass={
            !props.indeterminate
              ? glide
              : reduceMotion()
                ? "w-1/3 translate-x-full"
                : `w-1/3 transition-transform duration-700 ease-in-out ${end() ? "translate-x-[200%]" : "translate-x-0"}`
          }
          indicatorStyle={{ ...fillOf(), translate: props.indeterminate ? undefined : remaining(props.value) }}
        >
          <View style={{ flexDirection: "row", alignItems: "center", gap: size.gap }}>
            <Label label={props.label} detail={props.detail} />
            <Text style={{ fontSize: size.label, color: colors.muted, fontVariant: ["tabular-nums"], textAlign: "right" }}>
              {props.indeterminate ? "—" : `${props.value}%`}
            </Text>
          </View>
        </Progress>
      );
    }
    return (
      <Card icon="controls" title="Progress" subtitle="Transfers: the app owns the value">
        <View>
          <Transfer
            first
            label="Upload"
            detail={sent() >= 100 ? "24 photos sent" : running() ? "Sending 24 photos" : "24 photos to send"}
            value={sent()}
          />
          <Transfer label="Backup" detail="Preparing: its size is not known yet" value={0} indeterminate />
        </View>
        {/* As wide as their labels with a pointer; sharing the row on a touch screen. */}
        <View style={{ flexDirection: "row", gap: size.gap, marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label={running() ? "Pause" : sent() >= 100 ? "Send again" : "Send"}
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              if (running()) {
                stop();
                last.set("Upload: paused");
              } else start();
            }}
          />
          <Button
            label="Reset"
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            disabled={sent() === 0}
            onPress={() => {
              stop();
              sent.set(0);
              last.set("Upload: reset");
            }}
          />
        </View>
      </Card>
    );
  }

  function Uploads() {
    const sent = signal(FILES.map(() => 0));
    const running = signal(false);
    let timer: ReturnType<typeof setInterval> | undefined;
    const stop = () => {
      if (timer !== undefined) clearInterval(timer);
      timer = undefined;
      running.set(false);
    };
    cleanup(stop);
    const all = () => sent().every((v) => v >= 100);
    const start = () => {
      sent.set(FILES.map(() => 0));
      running.set(true);
      last.set("Uploads: started");
      // Each file at its own pace.
      timer = setInterval(() => {
        sent.set(sent().map((v, i) => Math.min(100, v + FILES[i].pace)));
        if (all()) {
          stop();
          last.set("Uploads: done");
        }
      }, 200);
    };
    /** A file: its name and size over a thin bar; its value turns into a check that pops in once it is sent. */
    function File(props: { name: string; size: string; icon: "photos" | "media" | "document"; value: number; first: boolean }) {
      const done = follow(() => (props.value >= 100 ? 1 : 0));
      const away = done.interpolate({ inputRange: [0, 1], outputRange: [1, 0] });
      const pop = popOn(() => props.value >= 100);
      return (
        <Progress
          value={props.value}
          style={{ ...row(props.first), flexDirection: "column-reverse", alignItems: "stretch", gap: size.padding }}
          trackStyle={rail(4)}
          indicatorClass={glide}
          indicatorStyle={{ ...fillOf(4, props.value >= 100 ? "up" : undefined), translate: remaining(props.value) }}
        >
          <View style={{ flexDirection: "row", alignItems: "center", gap: size.gap }}>
            <Icon name={props.icon} size={size.control} tint={colors.muted} />
            <Label label={props.name} detail={props.size} />
            <View style={{ width: 40, height: 20, alignItems: "flex-end", justifyContent: "center" }}>
              <View style={{ ...centered, alignItems: "flex-end", opacity: away }}>
                <Text style={{ fontSize: size.detail, color: colors.muted, fontVariant: ["tabular-nums"] }}>{`${props.value}%`}</Text>
              </View>
              <View style={{ ...centered, left: undefined, width: 20, opacity: done, transform: [{ scale: done.interpolate({ inputRange: [0, 1], outputRange: [0.5, 1] }) }, { scale: pop }] }}>
                <Icon name="check" size={size.check + 2} tint={toneColor("up")} weight={markWeight} />
              </View>
            </View>
          </View>
        </Progress>
      );
    }
    return (
      <Card title="Several files" subtitle="Each at its own pace; a check pops in as each is sent" right={<Badge label={`${sent().filter((v) => v >= 100).length} of ${FILES.length}`} tone={all() ? "up" : "neutral"} />}>
        <View>
          <Each of={FILES}>{(f, i) => <File name={f().name} size={f().size} icon={f().icon} value={sent()[i()]} first={i() === 0} />}</Each>
        </View>
        <View style={{ flexDirection: "row", marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label={running() ? "Uploading…" : all() ? "Upload again" : "Upload all"}
            size={size.switch}
            flex={pointer ? undefined : 1}
            disabled={running()}
            onPress={start}
          />
        </View>
      </Card>
    );
  }

  function Steps() {
    // 0 to 3: the step shown; 4: all done.
    const step = signal(1);
    const value = () => (Math.min(step(), STEPS.length - 1) / (STEPS.length - 1)) * 100;
    const DOT = 18;
    /** A step's dot: a ring around the current one, citrus with a check once done (a spring each), popping as it is reached. */
    function StepDot(props: { done: boolean; current: boolean }) {
      const done = follow(() => (props.done ? 1 : 0));
      const current = follow(() => (props.current ? 1 : 0));
      const pop = popOn(() => props.current);
      return (
        <View
          class="transition-colors duration-150 motion-reduce:transition-none"
          style={{
            width: DOT,
            height: DOT,
            borderRadius: DOT / 2,
            alignItems: "center",
            justifyContent: "center",
            borderWidth: 1,
            ...empty(),
            borderColor: props.current ? colors.accent : colors.fieldBorder,
            transform: [{ scale: pop }],
          }}
        >
          <Lit on={done} radius={DOT / 2} />
          <View style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: colors.accent, opacity: current }} />
          <View style={{ ...centered, opacity: done }}>
            <Icon name="check" size={10} tint={colors.onAccent} weight={markWeight} />
          </View>
        </View>
      );
    }
    return (
      <Card title="Steps" subtitle="A setup in four steps: the bar fills to the step you are on" right={<Badge label={step() >= STEPS.length ? "Done" : `Step ${step() + 1} of ${STEPS.length}`} tone={step() >= STEPS.length ? "up" : "neutral"} />}>
        {/* The bar runs from the first dot's middle to the last's; each label is centered under its dot. */}
        <View style={{ height: DOT + 24, marginHorizontal: 24 }}>
          <View style={{ position: "absolute", left: DOT / 2, right: DOT / 2, top: (DOT - 4) / 2 }}>
            <Progress value={value()} accessibilityLabel="Setup" trackStyle={rail(4)} indicatorClass={glide} indicatorStyle={{ ...fillOf(4), translate: remaining(value()) }} />
          </View>
          <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
            <Each of={STEPS}>
              {(name, i) => (
                <View style={{ width: DOT }}>
                  <StepDot done={i() < step()} current={i() === step()} />
                  <View style={{ position: "absolute", top: DOT + 6, left: (DOT - 64) / 2, width: 64, alignItems: "center" }}>
                    <Text style={{ fontSize: 12, fontWeight: i() === step() ? "600" : "400", color: i() <= step() ? colors.text : colors.faint }}>{name()}</Text>
                  </View>
                </View>
              )}
            </Each>
          </View>
        </View>
        <View style={{ flexDirection: "row", gap: size.gap, marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label="Previous step"
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            disabled={step() === 0}
            onPress={() => {
              step.set(Math.min(step(), STEPS.length) - 1);
              last.set(`Setup: ${STEPS[step()]}`);
            }}
          />
          <Button
            label={step() < STEPS.length - 1 ? "Next step" : step() === STEPS.length - 1 ? "Finish" : "Start over"}
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              step.set(step() >= STEPS.length ? 0 : step() + 1);
              last.set(step() >= STEPS.length ? "Setup: done" : `Setup: ${STEPS[step()]}`);
            }}
          />
        </View>
      </Card>
    );
  }

  function Meters() {
    const values = signal([14, 92, 60]);
    /** A meter's tone by its value: low battery red, a full disk amber. */
    const tone = (i: number, v: number): Tone | undefined => (i === 0 && v < 20 ? "danger" : i === 1 && v > 85 ? "warn" : undefined);
    return (
      <Card title="Sizes and tones" subtitle="Thin, regular and thick bars; a tone where the value calls for one">
        <View style={{ gap: size.gap + 4 }}>
          <Each of={METERS}>
            {(m, i) => (
              <Progress
                value={values()[i()]}
                accessibilityLabel={m().name}
                style={{ flexDirection: "column-reverse", alignItems: "stretch", gap: 6 }}
                trackStyle={rail(m().height)}
                indicatorClass={glide}
                indicatorStyle={{ ...fillOf(m().height, tone(i(), values()[i()])), translate: remaining(values()[i()]) }}
              >
                <View style={{ flexDirection: "row", alignItems: "baseline", gap: 8 }}>
                  <Text style={{ flex: 1, fontSize: size.label, color: colors.text }}>{m().name}</Text>
                  <Text
                    style={{ fontSize: size.detail, color: tone(i(), values()[i()]) ? toneColor(tone(i(), values()[i()])!) : colors.muted, fontVariant: ["tabular-nums"] }}
                  >
                    {m().note(values()[i()])}
                  </Text>
                </View>
              </Progress>
            )}
          </Each>
        </View>
        <View style={{ flexDirection: "row", marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label="Shuffle"
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              values.set(values().map(() => Math.round(5 + Math.random() * 93)));
              last.set(`Meters: ${values().join(", ")}`);
            }}
          />
        </View>
      </Card>
    );
  }

  function Levels() {
    const volume = signal(60);
    const price = signal<[number, number]>([100, 400]);
    const muted = signal(false);
    // The shared slider look (`sliderLook` and `sliderFill`, BJORK UI's
    // slider in the gallery's tones): an 8-point rail, a fill fading into a
    // 16 × 8 pill thumb, the value in a column after the rail; muted, it
    // fades to 35%. The spread holds only what never changes: a signal read
    // in it would make the slider again as it changes, mid-drag.
    /** A level: its label over the slider. */
    function Level(props: { label: string; detail: string; value?: string; first?: boolean; children: JSX.Element }) {
      return (
        <View style={{ ...row(!!props.first), flexDirection: "column", alignItems: "stretch", gap: size.padding }}>
          <View style={{ flexDirection: "row", alignItems: "center", gap: size.gap }}>
            <Label label={props.label} detail={props.detail} />
            <If when={props.value}>
              <Text style={{ fontSize: size.label, color: colors.muted, fontVariant: ["tabular-nums"], textAlign: "right" }}>{props.value}</Text>
            </If>
          </View>
          {props.children}
        </View>
      );
    }
    return (
      <Card icon="controls" title="Slider" subtitle="Levels and a range: the app owns the value">
        <View>
          <Level first label="Volume" detail={muted() ? "Muted" : "Media and alerts"} value={muted() ? "—" : `${volume()}%`}>
            {/* Quieter and louder at the ends; the louder one lights up near the top. */}
            <View style={{ flexDirection: "row", alignItems: "center", gap: 6, opacity: muted() ? 0.35 : 1 }}>
              <Icon name="volume-low" size={size.control} tint={colors.muted} />
              <Slider
                value={volume()}
                step={5}
                accessibilityLabel="Volume"
                onChange={({ value }) => volume.set(value as number)}
                onChangeEnd={({ value }) => last.set(`Volume: ${value}`)}
                disabled={muted()}
                rangeStyle={sliderFill(0, volume() / 100)}
                {...sliderLook()}
                style={{ flex: 1 }}
              />
              <Icon name="volume-high" size={size.control} tint={volume() >= 80 && !muted() ? colors.accent : colors.muted} />
            </View>
          </Level>
          <Level label="Price" detail={`${listings(price())} listings between the two`} value={`$${price()[0]} – $${price()[1]}`}>
            {/* How many listings each price has: the bars in the range light up as the thumbs move. */}
            <View style={{ flexDirection: "row", alignItems: "flex-end", gap: 2, height: 36, paddingHorizontal: pointer ? 7 : 10, marginBottom: -4 }}>
              <Each of={HISTOGRAM}>
                {(n, i) => (
                  <View
                    class="transition-colors duration-150 motion-reduce:transition-none"
                    style={{
                      flex: 1,
                      height: `${Math.round((n() / HISTOGRAM_MAX) * 100)}%`,
                      minHeight: 2,
                      borderTopLeftRadius: 2,
                      borderTopRightRadius: 2,
                      backgroundColor: inRange(i(), price()) ? colors.accentFill : colors.track,
                    }}
                  />
                )}
              </Each>
            </View>
            <Slider
              value={price()}
              min={0}
              max={500}
              step={10}
              startAccessibilityLabel="Minimum price"
              accessibilityLabel="Maximum price"
              onChange={({ value }) => price.set(value as [number, number])}
              onChangeEnd={({ value }) => last.set(`Price: ${(value as [number, number]).join(" to ")}`)}
              rangeStyle={sliderFill(price()[0] / 500, price()[1] / 500)}
              {...sliderLook()}
            />
          </Level>
        </View>
        <View style={{ flexDirection: "row", gap: size.gap, marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label={muted() ? "Unmute" : "Mute"}
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              muted.set(!muted());
              last.set(muted() ? "Volume: muted" : "Volume: unmuted");
            }}
          />
        </View>
      </Card>
    );
  }

  function Equalizer() {
    const bands = BANDS.map(() => signal(0));
    const preset = signal<"Flat" | "Bass" | "Vocal">("Flat");
    const apply = (name: "Flat" | "Bass" | "Vocal") => {
      preset.set(name);
      PRESETS[name].forEach((v, i) => bands[i].set(v));
      last.set(`Equalizer: ${name}`);
    };
    return (
      <Card title="Vertical" subtitle="An equalizer: five bands from −12 to +12 dB, and presets" right={<Segmented options={EQ_PRESETS} value={preset()} onChange={apply} size="small" />}>
        <View style={{ flexDirection: "row", justifyContent: "space-around", paddingTop: 4 }}>
          {BANDS.map((name, i) => (
            <View style={{ alignItems: "center", gap: 6 }}>
              <Text style={{ fontSize: size.detail, color: bands[i]() === 0 ? colors.faint : colors.text, fontVariant: ["tabular-nums"] }}>
                {`${bands[i]() > 0 ? "+" : ""}${bands[i]()}`}
              </Text>
              <Slider
                value={bands[i]()}
                min={-12}
                max={12}
                axis="vertical"
                accessibilityLabel={`${name} band`}
                onChange={({ value }) => bands[i].set(value as number)}
                onChangeEnd={({ value }) => last.set(`${name}: ${value} dB`)}
                rangeStyle={sliderFill(0, (bands[i]() + 12) / 24, true)}
                {...sliderLook({ vertical: true })}
              />
              <Text style={{ fontSize: size.detail, color: colors.muted }}>{name}</Text>
            </View>
          ))}
        </View>
      </Card>
    );
  }

  function Quantities() {
    const seats = signal<number | null>(2);
    const price = signal<number | null>(24.5);
    const locked = signal(false);
    // One recessed well per input: a stronger border under the pointer and
    // the keyboard's focus ring while its field edits (`focus-within:`);
    // the buttons light up under the pointer and while pressed (still with
    // reduced motion) and fade where a step does nothing (at the minimum
    // or maximum) or while the input is locked, without their hover.
    const frame = [
      pointer ? "h-8 rounded-md" : "h-11 rounded-lg",
      "flex-row items-stretch overflow-hidden border border-input bg-field",
      "enabled:hover:border-faint focus-within:outline-2 focus-within:outline-info",
    ].join(" ");
    const press = "items-center justify-center transition-colors duration-100 motion-reduce:transition-none enabled:hover:bg-accent active:bg-track disabled:opacity-35";
    const field = () => ({
      flex: 1,
      minWidth: 0,
      paddingHorizontal: 8,
      fontSize: size.label,
      color: colors.text,
      placeholderColor: colors.faint,
      fontVariant: ["tabular-nums" as const],
    });
    /** A step button's glyph: the gallery's icon, in the muted text color. */
    function Step(props: { name: "plus" | "minus" | "chevron-up" | "chevron-down"; size: number }) {
      return <Icon name={props.name} size={props.size} tint={colors.muted} />;
    }
    /** A quantity: its label and the input at the trailing edge. */
    function Amount(props: { label: string; detail: string; first?: boolean; children: JSX.Element }) {
      return (
        <View style={row(!!props.first)}>
          <Label label={props.label} detail={props.detail} />
          <View style={{ width: pointer ? 132 : 152 }}>{props.children}</View>
        </View>
      );
    }
    const show = (v: number | null) => (v === null ? "none" : `${v}`);
    return (
      <Card icon="controls" title="Number input" subtitle="Typed or stepped: the app owns the value">
        <View>
          <Amount first label="Seats" detail="1 to 8 per booking">
            <NumberInput
              value={seats()}
              min={1}
              max={8}
              disabled={locked()}
              accessibilityLabel="Seats"
              onChange={({ value: next }) => {
                seats.set(next);
                last.set(`Seats: ${show(next)}`);
              }}
              class={`${frame} disabled:opacity-50`}
              inputStyle={{ ...field(), textAlign: "center" }}
              decrementButtonClass={`${pointer ? "w-8" : "w-11"} ${press} border-r border-line`}
              incrementButtonClass={`${pointer ? "w-8" : "w-11"} ${press} border-l border-line`}
              decrementButton={<Step name="minus" size={pointer ? 14 : 16} />}
              incrementButton={<Step name="plus" size={pointer ? 14 : 16} />}
            />
          </Amount>
          <Amount label="Price" detail="Steps of 0.5">
            <NumberInput
              value={price()}
              min={0}
              step={0.5}
              placeholder="Free"
              accessibilityLabel="Price"
              controlsRight
              onChange={({ value: next }) => {
                price.set(next);
                last.set(`Price: ${show(next)}`);
              }}
              class={frame}
              inputStyle={{ ...field(), paddingLeft: 4 }}
              controlsClass={`${pointer ? "w-6" : "w-9"} border-l border-line`}
              incrementButtonClass={`flex-1 ${press} border-b border-line`}
              decrementButtonClass={`flex-1 ${press}`}
              incrementButton={<Step name="chevron-up" size={pointer ? 12 : 14} />}
              decrementButton={<Step name="chevron-down" size={pointer ? 12 : 14} />}
              prefix={
                <View style={{ height: "100%", justifyContent: "center", paddingLeft: 10 }}>
                  <Text style={{ fontSize: size.label, color: colors.muted, transform: [{ translateY: -1 }] }}>$</Text>
                </View>
              }
            />
          </Amount>
        </View>
        <View style={{ flexDirection: "row", gap: size.gap, marginTop: size.padding, alignSelf: pointer ? "flex-start" : "stretch" }}>
          <Button
            label={locked() ? "Unlock seats" : "Lock seats"}
            variant="secondary"
            size={size.switch}
            flex={pointer ? undefined : 1}
            onPress={() => {
              locked.set(!locked());
              last.set(locked() ? "Seats: locked" : "Seats: unlocked");
            }}
          />
        </View>
      </Card>
    );
  }

  function Verify() {
    const CODE = "246810";
    const code = signal("");
    const verdict = signal("");
    // The cell that takes the next digit shows the focus ring while the
    // code has the focus; the caret blinks there (steady with reduced
    // motion: the input's own).
    const cell = pointer ? "w-9 h-11 text-lg" : "w-11 h-13 text-xl";
    return (
      <Card icon="controls" title="One-time code" subtitle="Six digits: type or paste them">
        <View style={{ alignItems: "center", gap: size.gap, paddingVertical: size.padding }}>
          <OtpInput
            length={6}
            groups={2}
            value={code()}
            onChange={({ value: next }) => {
              code.set(next);
              verdict.set("");
              last.set(`Code: ${next || "empty"}`);
            }}
            onComplete={({ value }) => {
              verdict.set(value === CODE ? "Verified" : "That code is not right");
              last.set(`Code complete: ${value}`);
            }}
            accessibilityLabel="Verification code, 6 digits"
            class="flex-row items-center gap-3"
            groupClass="flex-row gap-1.5"
            cellClass={`${cell} items-center justify-center rounded-lg font-semibold text-foreground transition-colors motion-reduce:transition-none focus:outline-2 focus:outline-info focus:-outline-offset-2`}
            cellStyle={{ ...finish.inset, backgroundColor: colors.inset, borderWidth: 1, borderColor: colors.line }}
            caretClass="w-0.5 h-6 rounded-full"
            caretStyle={{ backgroundColor: colors.text }}
          />
          <Text style={{ fontSize: size.detail, color: verdict() === "Verified" ? toneColor("up") : verdict() ? toneColor("down") : colors.muted }}>
            {verdict() || `The code is ${CODE}`}
          </Text>
          <Button
            label="Clear"
            variant="secondary"
            size={size.switch}
            disabled={code() === ""}
            onPress={() => {
              code.set("");
              verdict.set("");
              last.set("Code: cleared");
            }}
          />
        </View>
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

  const cards: Record<Control, (() => JSX.Element)[]> = {
    Switch: [Settings, Quick],
    Checkbox: [Filter, Checklist, Consent, Channels, BoxStates],
    Radio: [Plans, Delivery, Reading],
    Tabs: [Sections, Project],
    Select: [Choices],
    Combobox: [Tags],
    DatePicker: [Dates],
    Collapsible: [Disclosure, Faq],
    Progress: [Transfers, Uploads, Steps, Meters],
    Slider: [Levels, Equalizer],
    NumberInput: [Quantities],
    OtpInput: [Verify],
  };
  return <DemoPage cards={[...cards[props.show], Last]} />;
}

export const SwitchDemo = () => <Controls show="Switch" />;
export const CheckboxDemo = () => <Controls show="Checkbox" />;
export const RadioDemo = () => <Controls show="Radio" />;
export const TabsDemo = () => <Controls show="Tabs" />;
export const SelectDemo = () => <Controls show="Select" />;
export const ComboboxDemo = () => <Controls show="Combobox" />;
export const DatePickerDemo = () => <Controls show="DatePicker" />;
export const CollapsibleDemo = () => <Controls show="Collapsible" />;
export const ProgressDemo = () => <Controls show="Progress" />;
export const SliderDemo = () => <Controls show="Slider" />;
export const NumberInputDemo = () => <Controls show="NumberInput" />;
export const OtpInputDemo = () => <Controls show="OtpInput" />;
