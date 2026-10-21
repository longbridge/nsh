import { mount, signal, Text, View } from "nsh";
import { Popover } from "nsh/base";
import { platform } from "nsh/device";
import { ui } from "./palette";

type Placement = "top" | "bottom" | "left" | "right";

/** A row of choices, one of them on. */
function Switch<T extends string | number>(props: { label: string; options: T[]; value: T; onChange: (value: T) => void }) {
  return (
    <View style={{ gap: 6 }}>
      <Text style={{ fontSize: 12, color: ui().muted }}>{props.label}</Text>
      <View style={{ flexDirection: "row", gap: 4, padding: 3, borderRadius: 10, ...ui().inset }}>
        <Each of={props.options}>
          {(option) => (
            <View
              onPress={() => props.onChange(option())}
              activeStyle={{ opacity: 0.6 }}
              style={{
                flex: 1,
                height: 30,
                borderRadius: 7,
                alignItems: "center",
                justifyContent: "center",
                ...(props.value === option() ? ui().raised : {}),
              }}
            >
              <Text style={{ fontSize: 13, fontWeight: "500", color: props.value === option() ? ui().text : ui().muted }}>{String(option())}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

function App() {
  const placement = signal<Placement>("bottom");
  const offset = signal(8);
  const events = signal<string[]>([]);
  const presses = signal(0);
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>popover</Text>
      <Text style={{ color: ui().muted }}>
        {platform.idiom === "desktop"
          ? "Open it, then click “Count” below: one click closes it and presses the button"
          : "Open it, then tap “Count” below: the tap only closes it"}
      </Text>

      <View style={{ flexDirection: "row", gap: 12 }}>
        <View style={{ flex: 3 }}>
          <Switch label="placement" options={["top", "bottom", "left", "right"] as Placement[]} value={placement()} onChange={(v) => placement.set(v)} />
        </View>
        <View style={{ flex: 2 }}>
          <Switch label="offset" options={[0, 8, 16]} value={offset()} onChange={(v) => offset.set(v)} />
        </View>
      </View>

      <View style={{ flex: 1, minHeight: 200, alignItems: "center", justifyContent: "center" }}>
        <Popover
          placement={placement()}
          offset={offset()}
          margin={12}
          popupClass="w-56 gap-2 rounded-xl p-4"
          popupStyle={ui().card}
          onOpenChange={(open, reason) => events.set([`${open ? "opened" : "closed"} · ${reason}`, ...events()].slice(0, 4))}
          trigger={
            <View activeStyle={{ opacity: 0.7 }} style={{ ...ui().accentFill, borderRadius: 10, paddingHorizontal: 18, height: 40, justifyContent: "center" }}>
              <Text style={{ fontSize: 15, fontWeight: "600", color: ui().onAccent }}>Order #1024</Text>
            </View>
          }
        >
          <Text style={{ fontSize: 15, fontWeight: "600", color: ui().text }}>Shipped yesterday</Text>
          <Text style={{ fontSize: 13, color: ui().muted }}>Arriving Friday. The trigger, a press outside and Escape close this.</Text>
          <View style={{ flexDirection: "row", gap: 6, alignItems: "center" }}>
            <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: ui().up }} />
            <Text style={{ fontSize: 12, color: ui().up }}>In transit</Text>
          </View>
        </Popover>
      </View>

      <View
        onPress={() => presses.update((n) => n + 1)}
        activeStyle={{ opacity: 0.7 }}
        style={{ ...ui().raised, borderRadius: 10, height: 40, alignItems: "center", justifyContent: "center" }}
      >
        <Text style={{ fontSize: 14, fontWeight: "500", color: ui().text }}>Count · pressed {presses()}×</Text>
      </View>

      <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: 92 }}>
        <Text style={{ fontSize: 12, color: ui().muted }}>onOpenChange(open, reason)</Text>
        <Each of={events()}>
          {(e, i) => <Text style={{ fontSize: 12, color: i() === 0 ? ui().text : ui().faint }}>{e()}</Text>}
        </Each>
      </View>
    </View>
  );
}

mount(App);
