// The fixture of the API's test suite for paint (docs/dev/compatibility.md):
// each case is a module of its own, shown alone when the script taps
// `id:case-<case>` (a one-point view over the corner, out of the layout);
// the steps after it are that case's. Kept with
// `cargo xtask api --keep api/cases/paint`.
import { type JSX, View, mount, signal } from "nsh";
import PaintBorders from "./paint-borders";
import PaintEffects from "./paint-effects";
import PaintInlineAlign from "./paint-inline-align";
import PaintMisc from "./paint-misc";
import PaintMotion from "./paint-motion";
import PaintShadows from "./paint-shadows";
import PaintText from "./paint-text";
import StateStyles from "./state-styles";
import StateChecked from "./state-checked";
import StateIndeterminate from "./state-indeterminate";

const cases: Record<string, () => JSX.Element> = {
  "paint-borders": PaintBorders,
  "paint-effects": PaintEffects,
  "paint-misc": PaintMisc,
  "paint-motion": PaintMotion,
  "paint-shadows": PaintShadows,
  "paint-text": PaintText,
  "state-styles": StateStyles,
  "state-checked": StateChecked,
  "state-indeterminate": StateIndeterminate,
  "paint-inline-align": PaintInlineAlign,
};
const shown = signal("");

mount(() => (
  <>
    {() => {
      const Case = cases[shown()];
      return Case ? <Case /> : null;
    }}
    <View style={{ position: "absolute", left: 0, top: 0, flexDirection: "row" }}>
      {Object.keys(cases).map(name => (
        <View testID={`case-${name}`} style={{ width: 1, height: 1 }} onPress={() => shown.set(name)} />
      ))}
    </View>
  </>
));
