import { Input, type InputProps, type JSX, mount, Scroll, signal, split, Text, View, type YNode } from "nsh";
import { desktop, size, tint, ui, wide } from "./palette";

/** A component: a function of its props, run once. */
function Badge(props: { label: string; color?: string }) {
  return (
    <View style={{ alignSelf: "flex-start", borderRadius: 999, paddingHorizontal: 10, paddingVertical: 4, backgroundColor: props.color ?? tint(ui().accent, 0.14) }}>
      <Text style={{ fontSize: 13, fontWeight: "600", color: ui().text }}>{props.label}</Text>
    </View>
  );
}

/** Children are whatever the caller put between the tags. */
function Card(props: { title: string; children?: JSX.Element }) {
  return (
    <View
      style={{ ...ui().card, borderRadius: 16, padding: 14, gap: 10 }}
    >
      <Text style={{ fontSize: 13, fontWeight: "600", color: ui().muted }}>{props.title}</Text>
      {props.children}
    </View>
  );
}

/** Own props are split off; the rest is spread onto the input and stays reactive. */
function Field(props: InputProps & { label: string }) {
  const [own, input] = split(props, ["label"]);
  return (
    <View style={{ gap: 4 }}>
      <Text style={{ fontSize: 13, color: ui().muted }}>{own.label}</Text>
      <Input
        style={
          wide()
            ? { ...ui().inset, fontSize: size.label, color: ui().text, placeholderColor: ui().faint, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 6, minHeight: size.control }
            : { ...ui().inset, fontSize: 16, color: ui().text, placeholderColor: ui().faint, borderRadius: 12, paddingHorizontal: 10, paddingVertical: 8 }
        }
        {...input}
      />
    </View>
  );
}

/** A button's box: a desktop control in a wide window. */
const button = () =>
  wide()
    ? { height: size.control, paddingHorizontal: 12, borderRadius: 8, justifyContent: "center" as const }
    : { borderRadius: 10, paddingHorizontal: 12, paddingVertical: 8 };

function App() {
  const name = signal("");
  const taps = signal(0);
  let field: YNode | undefined;

  return (
    <Scroll style={{ flex: 1, backgroundColor: ui().page }}>
      <View
        style={
          wide()
            ? { padding: 20, paddingTop: 24, gap: 16, display: "grid", gridTemplateColumns: "repeat(2, 1fr)", alignItems: "start" }
            : { padding: 20, paddingTop: 24, gap: 16 }
        }
      >
        <Card title="Props are live">
          <Badge label={`${desktop ? "Clicked" : "Tapped"} ${taps()} times`} color={taps() % 2 ? tint(ui().warn, 0.16) : tint(ui().accent, 0.14)} />
          <View onPress={() => taps.update((n) => n + 1)} activeStyle={{ opacity: 0.6 }} style={
              wide()
                ? { ...ui().primary, alignSelf: "flex-start", height: size.control, paddingHorizontal: 14, borderRadius: 8, justifyContent: "center" }
                : { ...ui().primary, borderRadius: 10, padding: 10 }
            }
          >
            <Text style={{ color: ui().onPrimary, textAlign: "center", fontWeight: "600", fontSize: wide() ? 13 : undefined }}>{desktop ? "Click" : "Tap"}</Text>
          </View>
        </Card>

        <Card title="split, spread and ref">
          <Field label="Your name" placeholder="Type here" value={name()} onChange={(t) => name.set(t)} ref={(node) => (field = node)} />
          <Text style={{ color: ui().text }}>{name() ? `Hello, ${name()}!` : "Nobody yet."}</Text>
          <View style={{ flexDirection: "row", gap: 8 }}>
            <View onPress={() => field?.focus()} activeStyle={{ opacity: 0.6 }} style={{ ...ui().accentFill, ...button() }}>
              <Text style={{ color: ui().onAccent, fontWeight: "600", fontSize: wide() ? 13 : undefined }}>Focus</Text>
            </View>
            <View onPress={() => (name.set(""), field?.blur())} activeStyle={{ opacity: 0.6 }} style={{ ...ui().raised, ...button() }}>
              <Text style={{ color: ui().text, fontSize: wide() ? 13 : undefined }}>Clear</Text>
            </View>
          </View>
        </Card>
      </View>
    </Scroll>
  );
}

mount(App);
