import { type JSX, mount, signal, Text, View } from "nsh";
import { Switch } from "nsh/base";
import { platform } from "nsh/device";
import { ui } from "./palette";

// The parts: geometry and motion in classes (`checked:` moves the thumb,
// `disabled:` dims the row, both following the switch), colors from the
// palette by the value the app owns.
const row = "flex-row-reverse items-center justify-between gap-3 px-4 py-3 active:opacity-70 disabled:opacity-40";
const track = "w-12 rounded-full p-0.5 transition-colors";
const thumb = "size-5.5 rounded-full transition-transform checked:translate-x-5";

function Setting(props: { label: string; detail: string; checked: boolean; disabled?: boolean; onChange: (next: boolean) => void }) {
  return (
    <Switch
      checked={props.checked}
      disabled={props.disabled}
      onChange={props.onChange}
      class={row}
      trackClass={track}
      trackStyle={props.checked ? ui().accentFill : ui().inset}
      thumbClass={thumb}
      thumbStyle={props.checked ? { backgroundColor: ui().onAccent } : ui().raised}
    >
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={{ fontSize: 15, fontWeight: "500", color: ui().text }}>{props.label}</Text>
        <Text style={{ fontSize: 12, color: ui().muted }}>{props.detail}</Text>
      </View>
    </Switch>
  );
}

function Panel(props: { title: string; children: JSX.Element; minHeight?: number }) {
  return (
    <View style={{ ...ui().inset, borderRadius: 12, padding: 12, gap: 4, minHeight: props.minHeight }}>
      <Text style={{ fontSize: 12, color: ui().muted }}>{props.title}</Text>
      {props.children}
    </View>
  );
}

function App() {
  const wifi = signal(true);
  const bluetooth = signal(false);
  const ask = signal(true);
  const events = signal<string[]>([]);
  const change = (name: string, value: { set: (v: boolean) => void }) => (next: boolean) => {
    value.set(next);
    events.set([`${name} → ${next}`, ...events()].slice(0, 3));
  };
  const line = () => <View style={{ height: 1, marginLeft: 16, backgroundColor: ui().line }} />;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>switch</Text>
      <Text style={{ color: ui().muted }}>
        {platform.idiom === "desktop" ? "Click a row, or Tab to a switch and press Space" : "Tap a row: the label toggles it too"}
      </Text>

      <View style={{ maxWidth: 390, gap: 14 }}>
        <View style={{ ...ui().card, borderRadius: 16, overflow: "hidden" }}>
          <Setting label="Wi-Fi" detail={wifi() ? "Connected to Studio" : "Off"} checked={wifi()} onChange={change("Wi-Fi", wifi)} />
          {line()}
          <Setting
            label="Ask to join networks"
            detail={wifi() ? "Known networks join on their own" : "Turn on Wi-Fi first"}
            checked={ask()}
            disabled={!wifi()}
            onChange={change("Ask to join", ask)}
          />
          {line()}
          <Setting label="Bluetooth" detail={bluetooth() ? "Discoverable as “Desk”" : "Off"} checked={bluetooth()} onChange={change("Bluetooth", bluetooth)} />
        </View>

        <Panel title="checked · disabled">
          <Text style={{ fontSize: 13, color: ui().text }}>
            Wi-Fi {wifi() ? "on" : "off"} · ask {ask() ? "on" : "off"}
            {wifi() ? "" : " (disabled)"} · Bluetooth {bluetooth() ? "on" : "off"}
          </Text>
        </Panel>
        <Panel title="onChange(checked)" minHeight={92}>
          <Each of={events()}>{(e, i) => <Text style={{ fontSize: 12, color: i() === 0 ? ui().text : ui().faint }}>{e()}</Text>}</Each>
        </Panel>
      </View>
    </View>
  );
}

mount(App);
