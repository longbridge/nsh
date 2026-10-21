import { mount, signal, Text, View } from "nsh";
import { ui } from "./palette";

// The app `nsh init` creates, with a reset button and a short history.
function App() {
  const count = signal(0);
  const history = signal<number[]>([]);

  const change = (delta: number) => {
    const next = count() + delta;
    count.set(next);
    history.update((h) => [next, ...h].slice(0, 4));
  };

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Text style={{ fontSize: 15, color: ui().muted }}>You tapped</Text>
      <Text style={{ fontSize: 56, fontWeight: "700", color: count() < 0 ? ui().down : ui().text }}>{count()}</Text>
      <View style={{ flexDirection: "row", gap: 12 }}>
        <View
          onPress={() => change(-1)}
          activeStyle={{ opacity: 0.6 }}
          role="button"
          style={{
            paddingHorizontal: 20,
            paddingVertical: 12,
            borderRadius: 10,
            ...ui().raised,
          }}
        >
          <Text style={{ color: ui().text, fontSize: 17 }}>−1</Text>
        </View>
        <View
          onPress={() => change(1)}
          activeStyle={{ opacity: 0.6 }}
          role="button"
          style={{
            paddingHorizontal: 20,
            paddingVertical: 12,
            borderRadius: 10,
            ...ui().accentFill,
          }}
        >
          <Text style={{ color: ui().onAccent, fontSize: 17, fontWeight: "600" }}>Tap</Text>
        </View>
      </View>
      <If when={history().length > 0}>
        <Text style={{ fontSize: 13, color: ui().muted }}>Recent: {history().join(", ")}</Text>
      </If>
      <View onPress={() => (count.set(0), history.set([]))} hitSlop={8}>
        <Text style={{ fontSize: 15, color: ui().accent }}>Reset</Text>
      </View>
    </View>
  );
}

mount(App);
