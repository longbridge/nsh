// The website's palette for its demos (website/DESIGN.md), in both
// appearances, and the layout they share: one column on a phone, the subject
// on a stage beside an inspector in a wide window. `ui()` reads
// `colorScheme()`, so a demo that reads it in its JSX follows the page's
// theme live. The playground inlines this file into a demo that imports it
// (it compiles one file), so it has no JSX and renames what it imports.
import type { JSX as __JSX } from "nsh";
import { Text as __Text, View as __View } from "nsh";
import { colorScheme as __scheme, platform as __platform, windowSize as __windowSize } from "nsh/device";

const __dark = {
  page: "#101113",
  text: "#eceef1",
  muted: "#8d9198",
  faint: "#5a5e65",
  border: "#2c2e33",
  line: "#1f2124",
  /** The accent as text, lines and small marks. */
  accent: "#c8f26d",
  /** Text on an accent fill. */
  onAccent: "#13160c",
  /** Text on a primary button. */
  onPrimary: "#111214",
  up: "#45d49a",
  down: "#ff6170",
  info: "#7ea8ff",
  warn: "#ffb547",
  violet: "#b79cff",
  /** Surfaces: a card, a row in a list, a recessed well or field, a raised
   * control, the primary button and an accent fill, with their borders. */
  card: {
    backgroundColor: "#18191c",
    backgroundImage: "linear-gradient(180deg, #1b1c20, #191a1d)",
    borderWidth: 1,
    borderColor: "#2c2e33",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.04), 0 16px 36px -20px rgba(0,0,0,0.75)",
  },
  row: {
    backgroundColor: "#18191c",
    backgroundImage: "linear-gradient(180deg, #1a1b1f, #191a1d)",
    borderWidth: 1,
    borderColor: "#2c2e33",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.05)",
  },
  inset: {
    backgroundColor: "#0c0d0f",
    borderWidth: 1,
    borderColor: "#1f2124",
    boxShadow: "inset 0 1px 3px rgba(0,0,0,0.55)",
  },
  raised: {
    backgroundColor: "#24262a",
    backgroundImage: "linear-gradient(180deg, #2b2d32, #24262a)",
    borderWidth: 1,
    borderColor: "#2c2e33",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.07), 0 1px 1px rgba(0,0,0,0.4), 0 6px 14px -7px rgba(0,0,0,0.7)",
  },
  primary: {
    backgroundColor: "#eceef1",
    backgroundImage: "linear-gradient(180deg, #fbfbfc, #e3e5e8)",
    boxShadow: "inset 0 1px 0 #ffffff, 0 1px 1px rgba(0,0,0,0.4), 0 8px 18px -8px rgba(0,0,0,0.75)",
  },
  accentFill: {
    backgroundColor: "#c8f26d",
    backgroundImage: "linear-gradient(180deg, #d6f78a, #c1ea5c)",
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0)",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.55), 0 1px 1px rgba(0,0,0,0.4), 0 8px 20px -9px rgba(200,242,109,0.4)",
  },
};

const __light: typeof __dark = {
  page: "#f3f3ef",
  text: "#15161a",
  muted: "#565a61",
  faint: "#8c9097",
  border: "#e1e1db",
  line: "#e9e9e4",
  accent: "#4a6a0a",
  onAccent: "#1b2207",
  onPrimary: "#ffffff",
  up: "#0f8a55",
  down: "#d43345",
  info: "#3d63d1",
  warn: "#b26a00",
  violet: "#6e56c4",
  card: {
    backgroundColor: "#ffffff",
    backgroundImage: "linear-gradient(180deg, #ffffff, #fdfdfc)",
    borderWidth: 1,
    borderColor: "rgba(17,17,10,0.08)",
    boxShadow: "inset 0 1px 0 #ffffff, 0 1px 1px rgba(20,20,10,0.04), 0 14px 32px -18px rgba(20,20,10,0.2)",
  },
  row: {
    backgroundColor: "#ffffff",
    backgroundImage: "linear-gradient(180deg, #fefefd, #fcfcfa)",
    borderWidth: 1,
    borderColor: "rgba(17,17,10,0.08)",
    boxShadow: "inset 0 1px 0 #ffffff, 0 1px 2px rgba(20,20,10,0.05)",
  },
  inset: {
    backgroundColor: "#ecece7",
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0.05)",
    boxShadow: "inset 0 1px 2px rgba(0,0,0,0.06)",
  },
  raised: {
    backgroundColor: "#f7f7f3",
    backgroundImage: "linear-gradient(180deg, #ffffff, #f6f6f2)",
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0.10)",
    boxShadow: "inset 0 1px 0 #ffffff, 0 1px 1px rgba(20,20,10,0.06), 0 5px 12px -6px rgba(20,20,10,0.18)",
  },
  primary: {
    backgroundColor: "#1d1e22",
    backgroundImage: "linear-gradient(180deg, #2e3035, #1b1c20)",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.16), 0 1px 1px rgba(20,20,10,0.2), 0 7px 14px -7px rgba(20,20,10,0.5)",
  },
  accentFill: {
    backgroundColor: "#c8f26d",
    backgroundImage: "linear-gradient(180deg, #d6f78a, #c1ea5c)",
    borderWidth: 1,
    borderColor: "#a7cf3f",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.6), 0 1px 1px rgba(80,110,10,0.2), 0 7px 14px -7px rgba(110,150,20,0.55)",
  },
};

/** The palette in the page's appearance; read it in JSX so the demo
 * follows theme changes. */
export const ui = () => (__scheme() === "light" ? __light : __dark);

/** `color` at `alpha` (0–1), for tinted badges and fills. */
export const tint = (color: string, alpha: number) => {
  const n = parseInt(color.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
};

/** A pressable under the pointer (`hoverStyle`): a whisper of the text
 * color over its surface, quieter than pressed and than selected. Leave it
 * off a disabled control. */
export const hoverFill = () => ({ backgroundColor: tint(ui().text, 0.05) });

/** The keyboard focus (`focusStyle`, `focus-visible:`): a 2-point ring of
 * `info` inside the view's edge, so a card's `overflow: hidden` and rounded
 * corners do not clip it. */
export const focusRing = () => ({ outlineStyle: "solid" as const, outlineWidth: 2, outlineColor: ui().info, outlineOffset: -2 });

/** A pointer and a keyboard: desktop density and copy ("Click", "Tab to it
 * and press Space"); touch density and copy ("Tap") otherwise. */
export const desktop = __platform.idiom === "desktop";

/** A window wide enough for the desktop layout (the site's Desktop frame is
 * 1024 points): the subject on a stage, its state and options beside it. */
export const wide = () => __windowSize().width >= 700;

/** Heights and type of the idiom (Design Guides, "Density tiers"): compact
 * with a pointer, 44-point targets on a touch screen. */
export const size = desktop
  ? { control: 32, controlSm: 28, row: 40, rowSm: 32, label: 14, detail: 12 }
  : { control: 44, controlSm: 44, row: 52, rowSm: 44, label: 15, detail: 12 };

/** The subject in a wide window: a bordered stage that takes the room and
 * centers its content, at most `width` across (440). */
export function Stage(props: { children?: __JSX.Element; width?: number; gap?: number }) {
  return __View({
    get style() {
      return {
        flex: 1,
        minWidth: 0,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: ui().line,
        backgroundColor: tint(ui().text, 0.02),
        padding: 24,
        alignItems: "center" as const,
        justifyContent: "center" as const,
      };
    },
    get children() {
      return __View({
        get style() {
          return { width: "100%" as const, maxWidth: props.width ?? 440, gap: props.gap ?? 14 };
        },
        get children() {
          return props.children;
        },
      });
    },
  });
}

/** The column beside a stage: the subject's state, its event log and the
 * options that tune it. */
export function Inspector(props: { children?: __JSX.Element }) {
  return __View({
    style: { width: 300, gap: 12 },
    get children() {
      return props.children;
    },
  });
}

/** The subject and its inspector: a stage beside an inspector column in a
 * wide window, one column `gap` apart otherwise (`grow`: the column fills
 * the page's height). */
export function Split(props: { children?: __JSX.Element; inspector?: __JSX.Element; gap?: number; width?: number; grow?: boolean }) {
  return __View({
    get style() {
      return wide() ? { flex: 1, flexDirection: "row" as const, gap: 16 } : { flex: props.grow ? 1 : 0, gap: props.gap ?? 14 };
    },
    get children() {
      if (!wide()) return [props.children, props.inspector];
      return [
        Stage({
          get width() {
            return props.width;
          },
          get gap() {
            return props.gap;
          },
          get children() {
            return props.children;
          },
        }),
        Inspector({
          get children() {
            return props.inspector;
          },
        }),
      ];
    },
  });
}

/** A recessed panel of state under a small title (`grow`: it takes the
 * column's free height). */
export function Panel(props: { title: string; children?: __JSX.Element; minHeight?: number; grow?: boolean }) {
  const title = __Text({
    get style() {
      return { fontSize: 12, color: ui().muted };
    },
    get children() {
      return props.title;
    },
  });
  return __View({
    get style() {
      return { ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: props.minHeight, flex: props.grow ? 1 : 0 };
    },
    get children() {
      return [title, props.children];
    },
  });
}

/** An event log, newest first: the newest line in the text color, older
 * ones faint. In a wide window it takes the inspector's free height. */
export function Log(props: { title: string; lines: string[]; minHeight?: number; grow?: boolean }) {
  return Panel({
    get title() {
      return props.title;
    },
    get minHeight() {
      return props.minHeight;
    },
    get grow() {
      return props.grow ?? wide();
    },
    get children() {
      return props.lines.map((line, i) =>
        __Text({
          get style() {
            return { fontSize: 12, color: i === 0 ? ui().text : ui().faint };
          },
          children: line,
        }),
      );
    },
  });
}

/** A labelled row of choices, one of them on: a demo's options. */
export function Segmented<T extends string | number>(props: { label?: string; options: readonly T[]; value: T; onChange: (value: T) => void }) {
  const option = (o: T) =>
    __View({
      onPress: () => props.onChange(o),
      activeStyle: { opacity: 0.6 },
      get style() {
        return {
          flex: 1,
          height: 30,
          borderRadius: 7,
          alignItems: "center" as const,
          justifyContent: "center" as const,
          ...(props.value === o ? ui().raised : {}),
        };
      },
      children: __Text({
        get style() {
          return { fontSize: 13, fontWeight: "500" as const, color: props.value === o ? ui().text : ui().muted };
        },
        children: String(o),
      }),
    });
  const label = props.label
    ? __Text({
        get style() {
          return { fontSize: 12, color: ui().muted };
        },
        children: props.label,
      })
    : null;
  const options = __View({
    get style() {
      return { flexDirection: "row" as const, gap: 4, padding: 3, borderRadius: 10, ...ui().inset };
    },
    children: props.options.map(option),
  });
  return __View({ style: { gap: 6 }, children: [label, options] });
}
