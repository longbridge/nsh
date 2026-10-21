import { mount, Overlay, signal, Text, View, type YNode } from "nsh";
import { Platform } from "nsh/device";
import { ui } from "./palette";

type Placement = "top" | "bottom" | "left" | "right";
type Align = "start" | "center" | "end";

/** A row of choices, one of them on. */
function Switch<T extends string>(props: { label: string; options: T[]; value: T; onChange: (value: T) => void }) {
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
              <Text style={{ fontSize: 13, fontWeight: "500", color: props.value === option() ? ui().text : ui().muted }}>{option()}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}

function App() {
  const open = signal(false);
  const placement = signal<Placement>("bottom");
  const align = signal<Align>("center");
  const position = signal("—");
  const closed = signal("—");
  let anchor: YNode | undefined;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>overlay</Text>
      <Text style={{ color: ui().muted }}>
        {Platform.isDesktop ? "Click the button, then click outside or press Escape" : "Tap the button, then tap outside"}
      </Text>

      <View style={{ flexDirection: "row", gap: 12 }}>
        <View style={{ flex: 3 }}>
          <Switch label="placement" options={["top", "bottom", "left", "right"]} value={placement()} onChange={(v) => (placement.set(v), open.set(true))} />
        </View>
        <View style={{ flex: 2 }}>
          <Switch label="align" options={["start", "center", "end"]} value={align()} onChange={(v) => (align.set(v), open.set(true))} />
        </View>
      </View>

      <View style={{ flex: 1, minHeight: 160, alignItems: "center", justifyContent: "center" }}>
        <View
          style={{
            width: 220,
            height: 72,
            overflow: "hidden",
            borderRadius: 14,
            borderWidth: 2,
            borderStyle: "dashed",
            borderColor: ui().faint,
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <View
            ref={anchor}
            onPress={() => open.set(!open())}
            activeStyle={{ opacity: 0.7 }}
            style={{ ...ui().accentFill, borderRadius: 10, paddingHorizontal: 18, height: 40, justifyContent: "center" }}
          >
            <Text style={{ fontSize: 15, fontWeight: "600", color: ui().onAccent }}>{open() ? "Close" : "Open"}</Text>
          </View>
          <If when={open()}>
            <Overlay
              anchor={anchor}
              placement={placement()}
              align={align()}
              offset={8}
              margin={12}
              onPosition={(e) =>
                position.set(
                  `${e.placement} at ${Math.round(e.bounds.origin.x)}, ${Math.round(e.bounds.origin.y)} · ${Math.round(e.bounds.size.width)}×${Math.round(e.bounds.size.height)} · room ${Math.round(e.available.width)}×${Math.round(e.available.height)}`,
                )
              }
              onPressOutside={(e) => {
                closed.set(`onPressOutside at ${Math.round(e.pagePosition.x)}, ${Math.round(e.pagePosition.y)}`);
                open.set(false);
              }}
              onCancel={() => {
                closed.set("onCancel");
                open.set(false);
              }}
              style={{
                ...ui().card,
                borderRadius: 12,
                padding: 14,
                gap: 4,
                // Narrow beside the button, so it fits on a phone's side.
                maxWidth: placement() === "left" || placement() === "right" ? 130 : 220,
              }}
            >
              <Text style={{ fontSize: 15, fontWeight: "600", color: ui().text }}>On top of everything</Text>
              <Text style={{ fontSize: 13, color: ui().muted }}>Its box clips the button, not this overlay. Pick a side above.</Text>
            </Overlay>
          </If>
        </View>
        <Text style={{ marginTop: 6, fontSize: 12, color: ui().faint }}>The dashed box has overflow: hidden</Text>
      </View>

      <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 6 }}>
        <Text style={{ fontSize: 12, color: ui().muted }}>
          onPosition <Text style={{ color: ui().text }}>{position()}</Text>
        </Text>
        <Text style={{ fontSize: 12, color: ui().muted }}>
          closed by <Text style={{ color: closed() === "—" ? ui().text : ui().accent }}>{closed()}</Text>
        </Text>
      </View>
    </View>
  );
}

mount(App);
