// A case of the API's test suite (docs/dev/compatibility.md): this domain
// declares capabilities without `window`, so the main window's commands
// (`nsh/desktop`'s `WindowRef`) reach nothing: asked to minimize, the
// window stays as it was. Its script checks the state the window reports.
import { Text } from "nsh";
import { currentWindow } from "nsh/desktop";

export default function WindowRefused() {
  const win = currentWindow();
  win.minimize();
  return <Text>{() => `Minimized: ${win.state()?.minimized ?? "unknown"}`}</Text>;
}
