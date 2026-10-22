import { mount, signal, Text, View } from "nsh";
import { desktop, size, ui } from "./palette";

// The app `nsh init` creates, with a reset button and a short history.

/** A button's box: a 32-point control with a pointer, a touch target otherwise. */
const button = () =>
  desktop
    ? { height: size.control, paddingHorizontal: 14, borderRadius: 8, justifyContent: "center" as const }
    : { paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10 };
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
      <Text style={{ fontSize: 15, color: ui().muted }}>{desktop ? "You clicked" : "You tapped"}</Text>
      <Text style={{ fontSize: 56, fontWeight: "700", color: count() < 0 ? ui().down : ui().text }}>{count()}</Text>
      <View style={{ flexDirection: "row", gap: 12 }}>
        <View
          onPress={() => change(-1)}
          activeStyle={{ opacity: 0.6 }}
          role="button"
          style={{
            ...button(),
            ...ui().raised,
          }}
        >
          <Text style={{ color: ui().text, fontSize: desktop ? 14 : 17 }}>−1</Text>
        </View>
        <View
          onPress={() => change(1)}
          activeStyle={{ opacity: 0.6 }}
          role="button"
          style={{
            ...button(),
            ...ui().accentFill,
          }}
        >
          <Text style={{ color: ui().onAccent, fontSize: desktop ? 14 : 17, fontWeight: "600" }}>{desktop ? "+1" : "Tap"}</Text>
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
