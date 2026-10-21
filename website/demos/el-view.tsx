import { mount, signal, Text, View } from "nsh";
import { ui } from "./palette";

function App() {
  const presses = signal(0);
  const size = signal("");
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 16 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>view</Text>

      <View
        onPress={() => presses.update((n) => n + 1)}
        activeStyle={{ opacity: 0.5 }}
        style={{
          ...ui().accentFill,
          borderRadius: 12,
          padding: 16,
          alignItems: "center",
        }}
      >
        <Text style={{ color: ui().onAccent, fontSize: 16, fontWeight: "600" }}>activeStyle opacity · pressed {presses()}×</Text>
      </View>

      <View
        onPress={() => presses.update((n) => n + 1)}
        activeStyle={{ backgroundColor: ui().border }}
        style={{ ...ui().card, backgroundImage: "none", borderRadius: 16, padding: 16 }}
      >
        <Text style={{ color: ui().text, fontSize: 16 }}>activeStyle background (tap and hold)</Text>
      </View>

      <View class="flex-row gap-3">
        <View class="flex-1 h-20 rounded-xl bg-[#ff6170] active:bg-[#d94d5b] items-center justify-center" onPress={() => {}}>
          <Text class="text-[#13160c] font-semibold">active:</Text>
        </View>
        <View class="flex-1 h-20 rounded-xl bg-[#45d49a] items-center justify-center shadow-lg">
          <Text class="text-[#0c0d0f] font-semibold">shadow-lg</Text>
        </View>
        <View
          class="flex-1 h-20 rounded-xl items-center justify-center"
          style={{ backgroundImage: `linear-gradient(135deg, ${ui().violet}, ${ui().info})` }}
        >
          <Text class="font-semibold" style={{ color: ui().page }}>gradient</Text>
        </View>
      </View>

      <View
        onLayout={(e) => size.set(`${Math.round(e.width)} × ${Math.round(e.height)} pt`)}
        style={{ flex: 1, borderRadius: 12, borderWidth: 2, borderStyle: "dashed", borderColor: ui().faint, alignItems: "center", justifyContent: "center" }}
      >
        <Text style={{ color: ui().muted }}>onLayout: {size()}</Text>
      </View>
    </View>
  );
}

mount(App);
