// A video playing the live stream every development host registers,
// `nsh/test-pattern`: the first frame's size once it loads, a second video
// of the same stream that goes on alone once the first stops showing it,
// and the stream set and cleared.
import { HostStream, mount, platform, signal, Text, Video, View } from "nsh";

function App() {
  const live = signal(true);
  const shown = signal(true);
  const first = signal("waiting");
  const second = signal("waiting");
  const stream = HostStream.fromHost("nsh/test-pattern");
  return (
    <View style={{ flex: 1, padding: 16, gap: 12 }}>
      <Text>{`Stream ${stream.name} ${live() ? "live" : "cleared"}`}</Text>
      <Text>{`On ${platform.os}`}</Text>
      <Video
        testID="first"
        srcObject={live() && shown() ? stream : null}
        muted
        style={{ height: 120, backgroundColor: "#111827", objectFit: "contain" }}
        onLoad={({ size }) => first.set(`${size.width}×${size.height}`)}
      />
      <Text>{`First ${first()}`}</Text>
      <Video
        testID="second"
        srcObject={live() ? stream : null}
        muted
        paused
        style={{ height: 60, backgroundColor: "#111827" }}
        onLoad={({ size }) => second.set(`${size.width}×${size.height}`)}
      />
      <Text>{`Second ${second()}`}</Text>
      <View role="button" accessibilityLabel="Toggle" onPress={() => live.set(!live())} style={{ height: 40 }}>
        <Text>Toggle</Text>
      </View>
      <View role="button" accessibilityLabel="Remove first" onPress={() => shown.set(false)} style={{ height: 40 }}>
        <Text>Remove first</Text>
      </View>
    </View>
  );
}

mount(() => <App />);
