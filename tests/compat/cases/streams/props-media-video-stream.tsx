// A case of the API's test suite (docs/dev/compatibility.md): a video given
// a live stream the embedding app registered (`srcObject`,
// `HostStream.fromHost`) beside its `src`: the handle's name, the
// stream set, cleared and set again, a second video of the stream that is
// `paused` (it shows the stream's first frame and reports `onLoad` for it,
// as a paused `<video>` does); a name that is not one is a `TypeError`, a
// name `capabilities.streams` does not grant is `ERR_ACCESS_DENIED`.
// Its script checks them (the platform's side of `srcObject` once
// nsh-compat reads it).
import { HostStream, Text, Video, View, signal } from "nsh";

function App() {
  const live = signal(true);
  let wrong = "none";
  try {
    HostStream.fromHost("");
  } catch (error) {
    wrong = error instanceof TypeError ? "TypeError" : String(error);
  }
  let denied = "none";
  try {
    HostStream.fromHost("compat-other");
  } catch (error) {
    denied = (error as { code?: string }).code ?? String(error);
  }
  const stream = HostStream.fromHost("compat-camera");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View testID="swap" onPress={() => live.set(!live())}>
        <Text>{`${live() ? "Live" : "Stopped"} ${stream.name}, empty ${wrong}, other ${denied}`}</Text>
      </View>
      <Video
        testID="live"
        src="https://example.com/fallback.mp4"
        srcObject={live() ? stream : null}
        muted
        style={{ width: 160, height: 90 }}
      />
      <Video
        testID="still"
        srcObject={live() ? stream : null}
        muted
        paused
        style={{ width: 160, height: 90 }}
      />
    </View>
  );
}

export default App;
