import { mount, signal, Text, View } from "nsh";
import { type OpenChangeReason, Tooltip } from "nsh/base";
import { platform } from "nsh/device";
import { ui } from "./palette";

/** An icon button with a hint of the app's look, above or below it. */
function Tool(props: {
  icon: string;
  label: string;
  hint: string;
  placement: "top" | "bottom";
  pinned: boolean;
  onPress: () => void;
  onOpenChange: (open: boolean, reason: OpenChangeReason) => void;
}) {
  const hovered = signal(false);
  return (
    <View style={{ alignItems: "center", gap: 6 }}>
      <View
        onPress={props.onPress}
        accessibilityLabel={props.label}
        activeStyle={{ opacity: 0.6 }}
        hoverStyle={{ borderColor: ui().accent }}
        style={{ ...ui().raised, width: 52, height: 52, borderRadius: 14, alignItems: "center", justifyContent: "center" }}
        tooltip={
          <Tooltip
            open={props.pinned || hovered()}
            placement={props.placement}
            offset={8}
            margin={8}
            delay={400}
            onOpenChange={(open, reason) => {
              hovered.set(open);
              props.onOpenChange(open, reason);
            }}
            style={{ backgroundColor: ui().text, borderRadius: 7, paddingHorizontal: 10, paddingVertical: 6 }}
          >
            <Text style={{ fontSize: 13, color: ui().page }}>{props.hint}</Text>
          </Tooltip>
        }
      >
        <Text style={{ fontSize: 22, color: ui().text }}>{props.icon}</Text>
      </View>
      <Text style={{ fontSize: 11, color: ui().faint }}>{props.placement}</Text>
    </View>
  );
}

function App() {
  const events = signal<string[]>([]);
  const pressed = signal("—");
  const pinned = signal(false);
  const log = (name: string) => (open: boolean, reason: OpenChangeReason) =>
    events.set([`${name} ${open ? "shown" : "hidden"} · ${reason}`, ...events()].slice(0, 6));
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>tooltip</Text>
      <Text style={{ color: ui().muted }}>
        {platform.idiom === "desktop"
          ? "Rest the pointer on a button; a press still presses it"
          : "Tooltips follow a pointer: pin them to see them here"}
      </Text>

      <View style={{ flex: 1, minHeight: 200, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 28 }}>
        <Tool icon="★" label="Star" hint="Add to favorites" placement="top" pinned={pinned()} onPress={() => pressed.set("Star")} onOpenChange={log("Star")} />
        <Tool icon="↗" label="Share" hint="Share with your team" placement="bottom" pinned={pinned()} onPress={() => pressed.set("Share")} onOpenChange={log("Share")} />
        <Tool icon="✕" label="Delete" hint="Move to the trash" placement="top" pinned={pinned()} onPress={() => pressed.set("Delete")} onOpenChange={log("Delete")} />
      </View>

      {/* The `tooltip` prop of any element: a text in the theme's bubble. */}
      <View style={{ flexDirection: "row", gap: 8 }}>
        <View
          tooltip="Shows every tooltip without a pointer"
          onPress={() => pinned.set(!pinned())}
          activeStyle={{ opacity: 0.6 }}
          style={{ ...ui().raised, flex: 1, height: 40, borderRadius: 10, alignItems: "center", justifyContent: "center" }}
        >
          <Text style={{ fontSize: 14, fontWeight: "500", color: ui().text }}>{pinned() ? "Unpin" : "Pin"}</Text>
        </View>
      </View>

      <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4 }}>
        <Text style={{ fontSize: 12, color: ui().muted }}>onPress</Text>
        <Text style={{ fontSize: 13, color: ui().text }}>{pressed()}</Text>
      </View>
      <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: 130 }}>
        <Text style={{ fontSize: 12, color: ui().muted }}>onOpenChange(open, reason)</Text>
        <Each of={events()}>{(e, i) => <Text style={{ fontSize: 12, color: i() === 0 ? ui().text : ui().faint }}>{e()}</Text>}</Each>
      </View>
    </View>
  );
}

mount(App);
