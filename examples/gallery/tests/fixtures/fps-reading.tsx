import { fpsEnabled, fpsReading, mount, setFpsEnabled, Text, View } from "nsh";

// The performance monitor turned on and off by the app, and what it reads:
// whether it measures, and its frame rate, once a second.
function App() {
  return (
    <View class="flex-1 bg-background" style={{ padding: 12, gap: 8 }}>
      <View role="button" testID="on" onPress={() => setFpsEnabled(true)}>
        <Text class="text-foreground">Measure</Text>
      </View>
      <View role="button" testID="off" onPress={() => setFpsEnabled(false)}>
        <Text class="text-foreground">Stop</Text>
      </View>
      <Text class="text-foreground">{`Enabled ${fpsEnabled()}`}</Text>
      <Text class="text-foreground">{`Reading ${fpsReading() ?? "none"}`}</Text>
    </View>
  );
}

mount(App);
