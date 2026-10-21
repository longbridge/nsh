// The fixture of the API's test suite for base (docs/dev/compatibility.md):
// each case is a module of its own, shown alone when the script taps
// `id:case-<case>` (a one-point view over the corner, out of the layout);
// the steps after it are that case's. Kept with
// `cargo xtask api --keep api/cases/base`.
import { type JSX, View, mount, signal } from "nsh";
import BaseStatusBar from "./base-status-bar";
import BaseAlertDialog from "./base-alert-dialog";
import BaseButton from "./base-button";
import BaseDialog from "./base-dialog";
import BaseDialogBackdrop from "./base-dialog-backdrop";
import BaseNavPage from "./base-nav-page";
import BaseNavStack from "./base-nav-stack";
import BaseSafeAreaKeyboard from "./base-safe-area-keyboard";
import BaseSelectionArea from "./base-selection-area";
import BaseSwitch from "./base-switch";
import BaseTextView from "./base-text-view";

const cases: Record<string, () => JSX.Element> = {
  "base-status-bar": BaseStatusBar,
  "base-alert-dialog": BaseAlertDialog,
  "base-button": BaseButton,
  "base-dialog": BaseDialog,
  "base-dialog-backdrop": BaseDialogBackdrop,
  "base-nav-page": BaseNavPage,
  "base-nav-stack": BaseNavStack,
  "base-safe-area-keyboard": BaseSafeAreaKeyboard,
  "base-text-view": BaseTextView,
  "base-selection-area": BaseSelectionArea,
  "base-switch": BaseSwitch,
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
