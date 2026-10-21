import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";
import { desktop, Inspector, Panel, size, Stage, tint, ui, wide } from "./palette";

function App() {
  const presses = signal(0);
  const pressing = signal(false);
  const disabled = signal(false);
  const bold = signal(true);
  const italic = signal(false);
  const tab = signal("inbox");
  const last = signal("–");
  // A pointer gets compact buttons as wide as their labels; a phone, 44-point ones.
  const chip = () =>
    desktop
      ? { ...ui().raised, borderRadius: 8, height: size.control, paddingHorizontal: 12, alignItems: "center" as const, justifyContent: "center" as const }
      : { ...ui().raised, borderRadius: 10, paddingHorizontal: 14, paddingVertical: 10, alignItems: "center" as const };
  const label = () => ({ color: ui().text, fontSize: desktop ? 13 : 15, fontWeight: "600" as const });

  const counter = () => (
    <Button
      onPress={() => {
        presses.update((n) => n + 1);
        last.set(`counter ${presses()}`);
      }}
      onPressDown={() => pressing.set(true)}
      onPressUp={() => pressing.set(false)}
      disabled={disabled()}
      style={
        desktop
          ? { ...ui().accentFill, alignSelf: "flex-start", borderRadius: 8, height: size.control, paddingHorizontal: 14, justifyContent: "center" }
          : { ...ui().accentFill, borderRadius: 12, padding: 14, alignItems: "center" }
      }
      activeStyle={{ opacity: 0.7 }}
      hoverStyle={{ borderColor: ui().text }}
      focusStyle={{ borderColor: ui().info, borderWidth: 2 }}
      disabledStyle={{ opacity: 0.35 }}
    >
      <Text style={{ color: ui().onAccent, fontSize: desktop ? 14 : 16, fontWeight: "600" }}>Pressed {presses()}×</Text>
    </Button>
  );
  const pressState = () => `${pressing() ? "onPressDown: pressing…" : "onPressUp: released"} · disabled ${String(disabled())}`;
  const disabler = () => (
    <Button
      onPress={() => disabled.set(!disabled())}
      role="switch"
      selected={disabled()}
      style={desktop ? { ...chip(), alignSelf: "flex-start" } : chip()}
      activeStyle={{ opacity: 0.7 }}
      selectedStyle={{ borderColor: ui().down }}
    >
      <Text style={label()}>{wide() ? (disabled() ? "Enable it" : "Disable it") : disabled() ? "Enable the button above" : "Disable the button above"}</Text>
    </Button>
  );
  const toggle = (name: string, on: () => boolean, flip: () => void, text: () => object, glyph: string) => (
    <Button
      selected={on()}
      onPress={flip}
      accessibilityLabel={name}
      style={{ ...chip(), width: desktop ? size.control : 48, paddingHorizontal: desktop ? 0 : 14 }}
      selectedStyle={{ backgroundColor: tint(ui().accent, 0.25), backgroundImage: "none", borderColor: ui().accent }}
      activeStyle={{ opacity: 0.7 }}
    >
      <Text style={text()}>{glyph}</Text>
    </Button>
  );
  const toggles = () => (
    <>
      {toggle("Bold", bold, () => bold.set(!bold()), () => ({ ...label(), fontWeight: "800" }), "B")}
      {toggle("Italic", italic, () => italic.set(!italic()), () => ({ ...label(), fontStyle: "italic" }), "I")}
    </>
  );
  const tabs = () => (
    <View style={{ ...ui().inset, flexDirection: "row", gap: 4, padding: desktop ? 3 : 4, borderRadius: desktop ? 10 : 12, alignSelf: desktop ? "flex-start" : "stretch" }}>
      {(["inbox", "archive", "spam"] as const).map((name) => (
        <Button
          role="tab"
          selected={tab() === name}
          onPress={() => {
            tab.set(name);
            last.set(`tab ${name}`);
          }}
          style={
            desktop
              ? { height: 26, paddingHorizontal: 14, borderRadius: 7, alignItems: "center", justifyContent: "center" }
              : { flex: 1, paddingVertical: 8, borderRadius: 9, alignItems: "center" }
          }
          selectedStyle={ui().raised}
          hoverStyle={{ backgroundColor: tint(ui().text, 0.06) }}
          activeStyle={{ opacity: 0.7 }}
        >
          <Text style={{ color: tab() === name ? ui().text : ui().muted, fontWeight: "600", fontSize: desktop ? 13 : undefined }}>{name}</Text>
        </Button>
      ))}
    </View>
  );
  const caption = (text: string) => <Text style={{ fontSize: 12, color: ui().muted }}>{text}</Text>;

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>button</Text>
      <Text style={{ color: ui().muted }}>{desktop ? "Click, hover, or Tab to a button and press Space" : "Tap; move your finger away to cancel"}</Text>

      {() =>
        wide() ? (
          <View style={{ flex: 1, flexDirection: "row", gap: 16 }}>
            <Stage width={360} gap={24}>
              <View style={{ gap: 8 }}>
                {caption("onPress, onPressDown, onPressUp")}
                <View style={{ flexDirection: "row", gap: 8 }}>
                  {counter()}
                  {disabler()}
                </View>
              </View>
              <View style={{ gap: 8 }}>
                {caption("selected: toggles")}
                <View style={{ flexDirection: "row", gap: 8 }}>{toggles()}</View>
              </View>
              <View style={{ gap: 8 }}>
                {caption('role="tab"')}
                {tabs()}
              </View>
            </Stage>
            <Inspector>
              <Panel title="press · disabled">
                <Text style={{ fontSize: 13, color: ui().text }}>{pressState()}</Text>
              </Panel>
              <Panel title="selected">
                <Text style={{ fontSize: 13, color: ui().text }}>
                  bold {String(bold())} · italic {String(italic())} · tab {tab()}
                </Text>
              </Panel>
              <Panel title="last onPress">
                <Text style={{ fontSize: 13, color: ui().up }}>{last()}</Text>
              </Panel>
            </Inspector>
          </View>
        ) : (
          <View style={{ gap: 12 }}>
            {counter()}
            <Text style={{ color: ui().muted, fontSize: 13 }}>{pressState()}</Text>

            {disabler()}

            <Text style={{ color: ui().muted, marginTop: 6 }}>selected: toggles</Text>
            <View style={{ flexDirection: "row", gap: 8 }}>
              {toggles()}
              <Text style={{ color: ui().muted, alignSelf: "center", flex: 1 }}>
                bold {String(bold())} · italic {String(italic())}
              </Text>
            </View>

            <Text style={{ color: ui().muted, marginTop: 6 }}>role="tab"</Text>
            {tabs()}
            <Text style={{ color: ui().up }}>last onPress: {last()}</Text>
          </View>
        )
      }
    </View>
  );
}

mount(App);
