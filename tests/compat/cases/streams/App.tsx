// The streams domain of the API's test apps (docs/dev/compatibility.md):
// its nsh.json declares `capabilities.streams`, so a case checks what the
// grant allows and what it refuses. Each case is a module of its own,
// shown alone when the script taps `id:case-<case>` (a one-point view over
// the corner, out of the layout); the steps after it are that case's.
import { type JSX, View, mount, signal } from "nsh";
import PropsMediaVideoStream from "./props-media-video-stream";

const cases: Record<string, () => JSX.Element> = {
  "props-media-video-stream": PropsMediaVideoStream,
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
