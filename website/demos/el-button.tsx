import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import { platform } from "nsh/device";
import { tint, ui } from "./palette";

function App() {
  const presses = signal(0);
  const pressing = signal(false);
  const disabled = signal(false);
  const bold = signal(true);
  const italic = signal(false);
  const tab = signal("inbox");
  const last = signal("–");
  const chip = () => ({ ...ui().raised, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 10, alignItems: "center" as const });
  const label = () => ({ color: ui().text, fontSize: 15, fontWeight: "600" as const });
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>button</Text>
      <Text style={{ color: ui().muted }}>
        {platform.idiom === "desktop" ? "Click, hover, or Tab to a button and press Space" : "Tap; move your finger away to cancel"}
      </Text>

      <View style={{ maxWidth: 390, gap: 12 }}>
        <Button
          onPress={() => {
            presses.update((n) => n + 1);
            last.set(`counter ${presses()}`);
          }}
          onPressDown={() => pressing.set(true)}
          onPressUp={() => pressing.set(false)}
          disabled={disabled()}
          style={{ ...ui().accentFill, borderRadius: 12, padding: 14, alignItems: "center" }}
          activeStyle={{ opacity: 0.7 }}
          hoverStyle={{ borderColor: ui().text }}
          focusStyle={{ borderColor: ui().info, borderWidth: 2 }}
          disabledStyle={{ opacity: 0.35 }}
        >
          <Text style={{ color: ui().onAccent, fontSize: 16, fontWeight: "600" }}>Pressed {presses()}×</Text>
        </Button>
        <Text style={{ color: ui().muted, fontSize: 13 }}>
          {pressing() ? "onPressDown: pressing…" : "onPressUp: released"} · disabled {String(disabled())}
        </Text>

        <Button
          onPress={() => disabled.set(!disabled())}
          role="switch"
          selected={disabled()}
          style={chip()}
          activeStyle={{ opacity: 0.7 }}
          selectedStyle={{ borderColor: ui().down }}
        >
          <Text style={label()}>{disabled() ? "Enable the button above" : "Disable the button above"}</Text>
        </Button>

        <Text style={{ color: ui().muted, marginTop: 6 }}>selected: toggles</Text>
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Button
            selected={bold()}
            onPress={() => bold.set(!bold())}
            accessibilityLabel="Bold"
            style={{ ...chip(), width: 48 }}
            selectedStyle={{ backgroundColor: tint(ui().accent, 0.25), backgroundImage: "none", borderColor: ui().accent }}
            activeStyle={{ opacity: 0.7 }}
          >
            <Text style={{ ...label(), fontWeight: "800" }}>B</Text>
          </Button>
          <Button
            selected={italic()}
            onPress={() => italic.set(!italic())}
            accessibilityLabel="Italic"
            style={{ ...chip(), width: 48 }}
            selectedStyle={{ backgroundColor: tint(ui().accent, 0.25), backgroundImage: "none", borderColor: ui().accent }}
            activeStyle={{ opacity: 0.7 }}
          >
            <Text style={{ ...label(), fontStyle: "italic" }}>I</Text>
          </Button>
          <Text style={{ color: ui().muted, alignSelf: "center", flex: 1 }}>
            bold {String(bold())} · italic {String(italic())}
          </Text>
        </View>

        <Text style={{ color: ui().muted, marginTop: 6 }}>role="tab"</Text>
        <View style={{ ...ui().inset, flexDirection: "row", gap: 4, padding: 4, borderRadius: 12 }}>
          {(["inbox", "archive", "spam"] as const).map((name) => (
            <Button
              role="tab"
              selected={tab() === name}
              onPress={() => {
                tab.set(name);
                last.set(`tab ${name}`);
              }}
              style={{ flex: 1, paddingVertical: 8, borderRadius: 9, alignItems: "center" }}
              selectedStyle={ui().raised}
              hoverStyle={{ backgroundColor: tint(ui().text, 0.06) }}
              activeStyle={{ opacity: 0.7 }}
            >
              <Text style={{ color: tab() === name ? ui().text : ui().muted, fontWeight: "600" }}>{name}</Text>
            </Button>
          ))}
        </View>
        <Text style={{ color: ui().up }}>last onPress: {last()}</Text>
      </View>
    </View>
  );
}

mount(App);
