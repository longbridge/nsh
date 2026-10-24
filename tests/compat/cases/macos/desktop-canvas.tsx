// A case of the API's test suite (docs/dev/compatibility.md): a canvas's
// `desktop={{ drawsAsynchronously }}`, run as on macOS (`platform macos`),
// beside one that leaves Core Animation its default.
import { Canvas, Text, View } from "nsh";

function App() {
  return (
    <View style={{ flex: 1 }}>
      <Text>Two canvases</Text>
      <Canvas
        testID="at-once"
        desktop={{ drawsAsynchronously: false }}
        style={{ height: 40 }}
        draw={(cx, size) => cx.fillRect(0, 0, size.width / 2, size.height)}
      />
      <Canvas
        testID="default"
        style={{ height: 40 }}
        draw={(cx, size) => cx.fillRect(0, 0, size.width, size.height / 2)}
      />
    </View>
  );
}

export default App;
