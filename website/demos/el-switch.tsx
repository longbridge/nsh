import { mount, signal, Text, View } from "nsh";
import { Switch } from "nsh/base";
import { Platform } from "nsh/device";
import { ui } from "./palette";

// The parts, styled with classes: `checked:` and `disabled:` follow the
// switch, `dark:` the page's theme.
const row = "flex-row-reverse items-center justify-between gap-3 px-4 py-3 disabled:opacity-40";
const track =
  "w-11 rounded-full bg-[#dcdcd6] p-0.5 transition-colors dark:bg-[#2c2e33] checked:bg-[#0f8a55] dark:checked:bg-[#45d49a]";
const thumb = "size-5 rounded-full bg-white shadow-sm transition-transform dark:bg-[#eceef1] checked:translate-x-5";

function Setting(props: { label: string; detail: string; checked: boolean; disabled?: boolean; onChange: (next: boolean) => void }) {
  return (
    <Switch
      checked={props.checked}
      disabled={props.disabled}
      onChange={props.onChange}
      class={row}
      trackClass={track}
      thumbClass={thumb}
    >
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={{ fontSize: 15, fontWeight: "500", color: ui().text }}>{props.label}</Text>
        <Text style={{ fontSize: 12, color: ui().muted }}>{props.detail}</Text>
      </View>
    </Switch>
  );
}

function App() {
  const wifi = signal(true);
  const bluetooth = signal(false);
  const ask = signal(true);
  const last = signal("–");
  const change = (name: string, value: { set: (v: boolean) => void }) => (next: boolean) => {
    value.set(next);
    last.set(`${name} → ${next}`);
  };
  const line = () => <View style={{ height: 1, marginLeft: 16, backgroundColor: ui().line }} />;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>switch</Text>
      <Text style={{ color: ui().muted }}>
        {Platform.isDesktop ? "Click a row, or Tab to a switch and press Space" : "Tap a row: the label toggles it too"}
      </Text>

      <View style={{ maxWidth: 390, gap: 12 }}>
        <View style={{ ...ui().card, borderRadius: 14, overflow: "hidden" }}>
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

        <Text style={{ color: ui().muted, fontSize: 13 }}>
          Wi-Fi {wifi() ? "on" : "off"} · ask {ask() ? "on" : "off"}
          {wifi() ? "" : " (disabled)"} · Bluetooth {bluetooth() ? "on" : "off"}
        </Text>
        <Text style={{ color: ui().up }}>last onChange: {last()}</Text>
      </View>
    </View>
  );
}

mount(App);
