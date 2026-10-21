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
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.06), 0 10px 28px rgba(0,0,0,0.38)",
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
    backgroundImage: "linear-gradient(180deg, #292b30, #26282c)",
    borderWidth: 1,
    borderColor: "#2c2e33",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.08), 0 1px 2px rgba(0,0,0,0.45)",
  },
  primary: {
    backgroundColor: "#eceef1",
    backgroundImage: "linear-gradient(180deg, #eeeff1, #e1e2e4)",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.9), 0 3px 10px rgba(0,0,0,0.4)",
  },
  accentFill: {
    backgroundColor: "#c8f26d",
    backgroundImage: "linear-gradient(180deg, #ceef7d, #c0e664)",
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0)",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.45), 0 3px 12px rgba(200,242,109,0.22)",
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
    backgroundImage: "linear-gradient(180deg, #fdfdfc, #fcfcfa)",
    borderWidth: 1,
    borderColor: "rgba(17,17,10,0.08)",
    boxShadow: "inset 0 1px 0 #ffffff, inset 0 -1px 0 rgba(0,0,0,0.03), 0 1px 2px rgba(20,20,10,0.06), 0 8px 24px rgba(20,20,10,0.07)",
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
    backgroundImage: "linear-gradient(180deg, #fbfbfa, #f8f8f5)",
    borderWidth: 1,
    borderColor: "rgba(0,0,0,0.10)",
    boxShadow: "inset 0 1px 0 #ffffff, 0 1px 2px rgba(20,20,10,0.08)",
  },
  primary: {
    backgroundColor: "#1d1e22",
    backgroundImage: "linear-gradient(180deg, #242429, #1c1e21)",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.14), 0 2px 6px rgba(20,20,10,0.22)",
  },
  accentFill: {
    backgroundColor: "#c8f26d",
    backgroundImage: "linear-gradient(180deg, #cdf276, #c6ed69)",
    borderWidth: 1,
    borderColor: "#a7cf3f",
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.55), 0 1px 3px rgba(80,110,10,0.18)",
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
