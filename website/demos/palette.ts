// The website's palette for its demos (website/DESIGN.md), in both
// appearances. `ui()` reads `colorScheme()`, so a demo that reads it in its
// JSX follows the page's theme live. The playground inlines this file into a
// demo that imports it (it compiles one file).
import { colorScheme as __scheme } from "nsh/device";

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
