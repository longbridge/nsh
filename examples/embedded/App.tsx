// The nsh app the embedding samples (examples/embed-ios, examples/embed-android)
// run. The badge is a native view of the host app, registered as
// `embed.badge` (`NshAppView.register(component:)`, `NshView.registerComponent`).
// The pill is drawn with the theme's tokens, which the host apps override
// with their brand (`NshAppView.setThemeOverrides`, `NshView.setThemeOverrides`).

import { mount, signal, Text, View, type YNode } from "nsh";
import { nativeComponent } from "nsh/native";
import { SafeAreaView } from "nsh/base";

/** The host app's badge: a label it draws, sized to its text. */
const Badge = nativeComponent<{ text: string; color?: string }, { tap: { count: number } }>("embed.badge", {
  props: ["text", "color"],
  events: ["tap"],
});

const colors = ["#8250df", "#0969da", "#1a7f37", "#cf222e"];

function App() {
  const taps = signal(0);
  let badge: YNode | undefined;
  return (
    <SafeAreaView style={{ flex: 1, padding: 20, gap: 16, backgroundColor: "#f6f8fa" }}>
      <Text style={{ fontSize: 22, fontWeight: "700" }}>Rendered by nsh</Text>
      <Text style={{ color: "#57606a" }}>
        The badge below is a native view of the host app: nsh lays it out, sends it props and gets its events.
      </Text>
      <View class="self-start rounded-(--radius) bg-primary px-3 py-2">
        <Text class="text-primary-foreground">Styled by the host app's theme</Text>
      </View>
      <Badge
        ref={(node) => (badge = node)}
        text={taps() === 0 ? "Tap the native badge" : `Tapped ${taps()}×`}
        color={colors[taps() % colors.length]}
        onTap={(e) => {
          taps.set(e.count);
          badge?.command("pulse");
        }}
        style={{ alignSelf: "flex-start", borderRadius: 8, overflow: "hidden" }}
      />
      <Text>{`JS saw ${taps()} taps`}</Text>
    </SafeAreaView>
  );
}

mount(App);
