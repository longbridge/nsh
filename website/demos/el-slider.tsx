import { type JSX, mount, signal, Text, View } from "nsh";
import { Slider, type SliderValue, Switch } from "nsh/base";
import { desktop, focusRing, hoverFill, Log, Panel, size, Split, ui } from "./palette";

// The parts: geometry and states in classes, colors from the theme's
// tokens. The track is taller than the rail so a finger finds it; the
// thumb grows a ring under the pointer and while dragged (a transition the
// reduced-motion setting stills) and takes the focus ring from the
// keyboard; a disabled slider dims and drops its hover. A pointer gets the
// compact sizes. Each row names its slider and shows the value above it;
// the equalizer's bands are vertical, their minimum at the bottom.
const track = desktop ? "h-5 justify-center" : "h-8 justify-center";
const verticalTrack = desktop ? "w-5 h-32 items-center" : "w-8 h-36 items-center";
const rail = "h-1.5 rounded-full bg-track";
const verticalRail = "w-1.5 h-full rounded-full bg-track";
const range = "rounded-full bg-brand disabled:bg-faint";
const thumb = (disabled: boolean) =>
  [
    desktop ? "size-4" : "size-6",
    "rounded-full border border-border bg-knob shadow-sm transition-shadow duration-150 motion-reduce:transition-none",
    "focus:outline-2 focus:outline-offset-2 focus:outline-info",
    disabled ? "" : "hover:ring-4 hover:ring-brand/20 active:ring-4 active:ring-brand/35",
  ].join(" ");

const hz = (v: number) => (v >= 1000 ? `${(v / 1000).toFixed(v >= 10000 ? 0 : 1)} kHz` : `${Math.round(v)} Hz`);
const show = (v: SliderValue) => (typeof v === "number" ? `${v}` : `${v[0]}–${v[1]}`);

function Row(props: { label: string; value: string; children: JSX.Element }) {
  return (
    <View class={desktop ? "gap-1 px-3 py-3" : "gap-2 px-4 py-4"}>
      <View class="flex-row items-baseline gap-3">
        <Text style={{ flex: 1, fontSize: size.label, fontWeight: "500", color: ui().text }}>{props.label}</Text>
        <Text style={{ fontSize: size.label, color: ui().muted, fontVariant: ["tabular-nums"] }}>{props.value}</Text>
      </View>
      {props.children}
    </View>
  );
}

function App() {
  const volume = signal(40);
  const price = signal<[number, number]>([200, 800]);
  const frequency = signal(440);
  const bands = [signal(0), signal(3), signal(-2)];
  const disabled = signal(false);
  const events = signal<string[]>([]);
  const log = (name: string) => (next: SliderValue) => events.set([`${name} → ${show(next)}`, ...events()].slice(0, 5));
  const line = () => <View style={{ height: 1, marginLeft: desktop ? 12 : 16, backgroundColor: ui().line }} />;
  const parts = () => ({
    disabled: disabled(),
    class: "disabled:opacity-50",
    trackClass: track,
    indicatorClass: rail,
    rangeClass: range,
    thumbClass: thumb(disabled()),
  });
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>slider</Text>
      <Text style={{ color: ui().muted }}>
        {desktop ? "Drag a thumb or click the track, or Tab to a thumb and use the arrow keys" : "Drag a thumb, or tap the track"}
      </Text>

      <Split
        gap={14}
        inspector={
          <>
            <Panel title="value · disabled">
              <Text style={{ fontSize: 13, color: ui().text, fontVariant: ["tabular-nums"] }}>
                volume {volume()} · price [{price()[0]}, {price()[1]}] · {Math.round(frequency())}
                {disabled() ? " (disabled)" : ""}
              </Text>
              <Switch
                checked={disabled()}
                onChange={(next) => disabled.set(next)}
                class={`-mx-2 flex-row-reverse items-center justify-between gap-3 rounded-lg px-2 ${desktop ? "py-1" : "py-2"} active:opacity-70`}
                hoverStyle={hoverFill()}
                focusStyle={focusRing()}
                trackClass={`${desktop ? "w-9" : "w-12"} rounded-full p-0.5 transition-colors motion-reduce:transition-none`}
                trackStyle={disabled() ? ui().accentFill : ui().inset}
                thumbClass={desktop ? "size-4 rounded-full transition-transform motion-reduce:transition-none checked:translate-x-4" : "size-5.5 rounded-full transition-transform motion-reduce:transition-none checked:translate-x-5"}
                thumbStyle={disabled() ? { backgroundColor: ui().onAccent } : ui().raised}
              >
                <Text style={{ flex: 1, fontSize: size.label, color: ui().text }}>Disabled</Text>
              </Switch>
            </Panel>
            <Log title="onChangeEnd(value)" lines={events()} minHeight={92} />
          </>
        }
      >
        <View style={{ ...ui().card, borderRadius: 16, overflow: "hidden" }}>
          <Row label="Volume" value={`${volume()}%`}>
            <Slider
              value={volume()}
              onChange={(next) => volume.set(next as number)}
              onChangeEnd={log("volume")}
              accessibilityLabel="Volume"
              {...parts()}
            />
          </Row>
          {line()}
          <Row label="Price" value={`$${price()[0]} – $${price()[1]}`}>
            <Slider
              value={price()}
              min={0}
              max={1000}
              step={10}
              onChange={(next) => price.set(next as [number, number])}
              onChangeEnd={log("price")}
              startAccessibilityLabel="Minimum price"
              accessibilityLabel="Maximum price"
              {...parts()}
            />
          </Row>
          {line()}
          <Row label="Frequency" value={hz(frequency())}>
            <Slider
              value={frequency()}
              min={20}
              max={20000}
              scale="logarithmic"
              onChange={(next) => frequency.set(next as number)}
              onChangeEnd={log("frequency")}
              accessibilityLabel="Frequency"
              {...parts()}
            />
          </Row>
          {line()}
          <Row label="Equalizer" value="dB">
            <View class="flex-row justify-around pt-1">
              {["Bass", "Mid", "Treble"].map((name, i) => (
                <View class="items-center gap-2">
                  <Slider
                    value={bands[i]()}
                    min={-12}
                    max={12}
                    axis="vertical"
                    onChange={(next) => bands[i].set(next as number)}
                    onChangeEnd={log(name.toLowerCase())}
                    accessibilityLabel={name}
                    {...parts()}
                    trackClass={verticalTrack}
                    indicatorClass={verticalRail}
                  />
                  <Text style={{ fontSize: size.detail, color: ui().muted, fontVariant: ["tabular-nums"] }}>
                    {`${name} ${bands[i]() > 0 ? "+" : ""}${bands[i]()}`}
                  </Text>
                </View>
              ))}
            </View>
          </Row>
        </View>
      </Split>
    </View>
  );
}

mount(App);
