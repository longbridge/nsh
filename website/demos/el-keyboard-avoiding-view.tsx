import { Input, mount, Scroll, signal, Text, View } from "nsh";
import { Button, Keyboard, Switch, KeyboardAvoidingView, keyboardFrame } from "nsh/base";
import { desktop, tint, ui } from "./palette";

function App() {
  const draft = signal("");
  const messages = signal(["Type below: the bar stays above the keyboard."]);
  const behavior = signal<"padding" | "height">("padding");
  const enabled = signal(true);
  const send = () => {
    if (!draft().trim()) return;
    messages.update((list) => [...list, draft().trim()]);
    draft.set("");
  };
  const chip = (on: boolean) => ({
    ...(on ? ui().accentFill : ui().raised),
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 6,
  });
  const chipText = (on: boolean) => ({ color: on ? ui().onAccent : ui().text, fontSize: 13, fontWeight: "600" as const });
  return (
    <KeyboardAvoidingView
      enabled={enabled()}
      behavior={behavior()}
      style={{ flex: 1, backgroundColor: behavior() === "height" ? tint(ui().info, 0.12) : ui().page }}
    >
      <View style={{ padding: 20, paddingTop: 24, paddingBottom: 8, gap: 8 }}>
        <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>keyboard-avoiding-view</Text>
        <Text style={{ color: ui().muted }}>
          {desktop
            ? "Desktops type with a hardware keyboard: its height stays 0 and the view keeps its layout. Open this on a phone to see it move."
            : "Tap the message field: the bar moves up with the keyboard."}
        </Text>
        <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
          <Button onPress={() => behavior.set("padding")} selected={behavior() === "padding"} style={chip(behavior() === "padding")}>
            <Text style={chipText(behavior() === "padding")}>padding</Text>
          </Button>
          <Button onPress={() => behavior.set("height")} selected={behavior() === "height"} style={chip(behavior() === "height")}>
            <Text style={chipText(behavior() === "height")}>height</Text>
          </Button>
          <Switch
            checked={enabled()}
            onChange={(next) => enabled.set(next)}
            class="flex-row items-center gap-2 px-1 active:opacity-70"
            trackClass="w-9 rounded-full p-0.5 transition-colors"
            trackStyle={enabled() ? ui().accentFill : ui().inset}
            thumbClass="size-4 rounded-full transition-transform checked:translate-x-4"
            thumbStyle={enabled() ? { backgroundColor: ui().onAccent } : ui().raised}
          >
            <Text style={{ color: ui().text, fontSize: 13, fontWeight: "600" }}>enabled {String(enabled())}</Text>
          </Switch>
        </View>
        <Text style={{ color: ui().accent, fontSize: 13 }}>
          keyboardFrame(): height {Math.round(keyboardFrame().height)} · duration {keyboardFrame().duration.toFixed(2)} s
        </Text>
      </View>

      <Scroll style={{ flex: 1 }} keyboardDismissMode="interactive">
        <View style={{ padding: 20, paddingTop: 4, gap: 8 }}>
          <Each of={messages()}>
            {(text) => (
              <View style={{ ...ui().row, alignSelf: "flex-start", maxWidth: 320, borderRadius: 14, paddingHorizontal: 12, paddingVertical: 8 }}>
                <Text style={{ color: ui().text }}>{text()}</Text>
              </View>
            )}
          </Each>
        </View>
      </Scroll>

      <View style={{ flexDirection: "row", gap: 8, padding: 12, paddingHorizontal: desktop ? 20 : 12, borderTopWidth: 1, borderTopColor: ui().border, backgroundColor: ui().page }}>
        <Input
          value={draft()}
          onChange={(text) => draft.set(text)}
          onSubmit={send}
          testID="message"
          returnKeyType="send"
          placeholder="Message"
          style={{ ...ui().inset, flex: 1, fontSize: 16, color: ui().text, placeholderColor: ui().faint, borderRadius: 10, paddingHorizontal: 12, paddingVertical: 8 }}
        />
        <Button onPress={send} style={{ ...ui().primary, borderRadius: 10, paddingHorizontal: 14, justifyContent: "center" }} activeStyle={{ opacity: 0.7 }}>
          <Text style={{ color: ui().onPrimary, fontWeight: "600" }}>Send</Text>
        </Button>
        <Button onPress={() => Keyboard.dismiss()} style={{ ...ui().raised, borderRadius: 10, paddingHorizontal: 12, justifyContent: "center" }} activeStyle={{ opacity: 0.7 }}>
          <Text style={{ color: ui().text, fontWeight: "600" }}>Done</Text>
        </Button>
      </View>
    </KeyboardAvoidingView>
  );
}

mount(App);
