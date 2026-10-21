import { mount, signal, Text, View } from "nsh";
import { Sheet } from "nsh/base";
import { platform } from "nsh/device";
import { ui } from "./palette";

type Side = "bottom" | "top" | "left" | "right";

/** The panel's corners and size across, by the edge it is on. */
function panel(side: Side) {
  const r = 20;
  const across = side === "left" || side === "right" ? { width: 280 } : {};
  const corners = {
    bottom: { borderTopLeftRadius: r, borderTopRightRadius: r, paddingBottom: 32 },
    top: { borderBottomLeftRadius: r, borderBottomRightRadius: r, paddingTop: 32 },
    left: { borderTopRightRadius: r, borderBottomRightRadius: r },
    right: { borderTopLeftRadius: r, borderBottomLeftRadius: r },
  }[side];
  return { ...ui().card, padding: 20, gap: 12, ...across, ...corners };
}

function App() {
  const open = signal(false);
  const side = signal<Side>("bottom");
  const events = signal<string[]>([]);
  const log = (e: string) => events.set([e, ...events()].slice(0, 6));
  const show = (s: Side) => {
    side.set(s);
    open.set(true);
    log(`open · side ${s}`);
  };
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>sheet</Text>
      <Text style={{ color: ui().muted }}>
        {platform.idiom === "desktop" ? "Open a side; click the backdrop or press Escape to close it" : "Open a side; tap the backdrop or Android's back to close it"}
      </Text>

      <View style={{ flex: 1, minHeight: 200, alignItems: "center", justifyContent: "center" }}>
        <View style={{ width: 240, gap: 8 }}>
          <Each of={["top", "left", "right", "bottom"] as Side[]}>
            {(s) => (
              <View
                onPress={() => show(s())}
                activeStyle={{ opacity: 0.7 }}
                style={{
                  ...(s() === side() ? ui().accentFill : ui().raised),
                  height: 42,
                  borderRadius: 10,
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <Text style={{ fontSize: 14, fontWeight: "600", color: s() === side() ? ui().onAccent : ui().text }}>side="{s()}"</Text>
              </View>
            )}
          </Each>
        </View>
      </View>

      <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: 130 }}>
        <Text style={{ fontSize: 12, color: ui().muted }}>onOpenChange(open, reason) · onCancel</Text>
        <Each of={events()}>{(e, i) => <Text style={{ fontSize: 12, color: i() === 0 ? ui().text : ui().faint }}>{e()}</Text>}</Each>
      </View>

      <Sheet
        open={open()}
        side={side()}
        onOpenChange={(next, reason) => {
          open.set(next);
          log(`onOpenChange · ${next ? "opened" : "closed"} · ${reason}`);
        }}
        onCancel={() => log("onCancel")}
        popupStyle={panel(side())}
        backdropStyle={{ backgroundColor: "rgba(0,0,0,0.45)" }}
      >
        <Text style={{ fontSize: 17, fontWeight: "700", color: ui().text }}>Filters</Text>
        <Text style={{ fontSize: 14, color: ui().muted }}>
          A panel at the {side()} edge. {platform.idiom === "desktop" ? "The backdrop and Escape close it." : "The backdrop and Android's back close it."}
        </Text>
        <View
          onPress={() => {
            open.set(false);
            log("Done pressed");
          }}
          activeStyle={{ opacity: 0.7 }}
          style={{ ...ui().primary, height: 42, borderRadius: 10, alignItems: "center", justifyContent: "center" }}
        >
          <Text style={{ fontSize: 14, fontWeight: "600", color: ui().onPrimary }}>Done</Text>
        </View>
      </Sheet>
    </View>
  );
}

mount(App);
