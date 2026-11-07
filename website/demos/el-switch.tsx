import { mount, signal, Text, View } from "nsh";
import { Switch } from "nsh/base";
import { desktop, focusRing, hoverFill, Log, Panel, size, Split, switchLook, ui } from "./palette";

// The track and the thumb take the shared switch look (`switchLook`): the
// knob springs across by the value the app owns, as far from the track's
// edge on as off, and lands at once with reduced motion. The row is the
// switch itself: `disabled:` dims it, the pointer's hover and the keyboard's
// focus ring come from the palette.
const row = `flex-row-reverse items-center justify-between gap-3 ${desktop ? "px-3 py-2" : "px-4 py-3"} active:opacity-70 disabled:opacity-40`;

function Setting(props: { label: string; detail: string; checked: boolean; disabled?: boolean; onChange: (e: { checked: boolean }) => void }) {
  return (
    <Switch
      checked={props.checked}
      disabled={props.disabled}
      onChange={props.onChange}
      class={row}
      hoverStyle={props.disabled ? undefined : hoverFill()}
      focusStyle={focusRing()}
      {...switchLook(() => props.checked)}
    >
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={{ fontSize: size.label, fontWeight: "500", color: ui().text }}>{props.label}</Text>
        <Text style={{ fontSize: size.detail, color: ui().muted }}>{props.detail}</Text>
      </View>
    </Switch>
  );
}

function App() {
  const wifi = signal(true);
  const bluetooth = signal(false);
  const ask = signal(true);
  const events = signal<string[]>([]);
  const change = (name: string, value: { set: (v: boolean) => void }) => ({ checked }: { checked: boolean }) => {
    value.set(checked);
    events.set([`${name} → ${checked}`, ...events()].slice(0, 3));
  };
  const line = () => <View style={{ height: 1, marginLeft: desktop ? 12 : 16, backgroundColor: ui().line }} />;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>switch</Text>
      <Text style={{ color: ui().muted }}>
        {desktop ? "Click a row, or Tab to a switch and press Space" : "Tap a row: the label toggles it too"}
      </Text>

      <Split
        gap={14}
        inspector={
          <>
            <Panel title="checked · disabled">
              <Text style={{ fontSize: 13, color: ui().text }}>
                Wi-Fi {wifi() ? "on" : "off"} · ask {ask() ? "on" : "off"}
                {wifi() ? "" : " (disabled)"} · Bluetooth {bluetooth() ? "on" : "off"}
              </Text>
            </Panel>
            <Log title="onChange({ checked })" lines={events()} minHeight={92} />
          </>
        }
      >
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
      </Split>
    </View>
  );
}

mount(App);
