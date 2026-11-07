import { cleanup, mount, signal, Text, View } from "nsh";
import { reduceMotion } from "nsh";
import { Button, Progress, Switch } from "nsh/base";
import { button, buttonText, desktop, Log, Panel, size, Split, switchLook, ui, controlSize, listRow, plainRow, RowLabel, sectionSurface } from "./palette";

// The parts: geometry and motion in classes, colors from the palette. The
// label row comes first (`flex-col-reverse`: the children follow the track),
// its value tabular and at the trailing edge. An indeterminate indicator is a
// third of the track that sweeps from end to end (the app flips its
// translation, a transition moves it), and rests in the middle with reduced
// motion. A determinate one is the whole track, slid back by what is left to
// do, so a new value glides there (the track clips it). The rows and the
// switch's row are the shared ones (`plainRow`, `listRow`). Buttons are as
// wide as their labels with a pointer.
const track = "h-1.5 overflow-hidden rounded-full";
const SWEEP = 800;
const indicator = (indeterminate: boolean, end: boolean) =>
  !indeterminate
    ? "h-full w-full rounded-full transition-transform duration-180 ease-[cubic-bezier(0.2,0,0,1)] motion-reduce:transition-none"
    : reduceMotion()
      ? "h-full w-1/3 rounded-full translate-x-full"
      : `h-full w-1/3 rounded-full transition-transform duration-700 ease-in-out ${end ? "translate-x-[200%]" : "translate-x-0"}`;

function Transfer(props: { label: string; detail: string; value: number; indeterminate?: boolean; end: boolean; first?: boolean }) {
  return (
    <Progress
      value={props.value}
      indeterminate={props.indeterminate}
      style={{ ...plainRow({ first: props.first }), flexDirection: "column-reverse", alignItems: "stretch", gap: controlSize.padding }}
      trackClass={track}
      trackStyle={ui().inset}
      indicatorClass={indicator(!!props.indeterminate, props.end)}
      indicatorStyle={{ backgroundColor: ui().accent, translate: props.indeterminate ? undefined : `${props.value - 100}% 0` }}
    >
      <View class="flex-row items-center gap-3">
        <RowLabel label={props.label} detail={props.detail} />
        <Text style={{ fontSize: controlSize.label, color: ui().muted, fontVariant: ["tabular-nums"], textAlign: "right" }}>
          {props.indeterminate ? "—" : `${Math.round(props.value)}%`}
        </Text>
      </View>
    </Progress>
  );
}

function Action(props: { label: string; disabled?: boolean; onPress: () => void }) {
  return (
    <Button
      onPress={props.onPress}
      disabled={props.disabled}
      {...button("secondary", { disabled: () => !!props.disabled, style: desktop ? {} : { flex: 1 } })}
    >
      <Text style={buttonText()}>{props.label}</Text>
    </Button>
  );
}

function App() {
  const sent = signal(35);
  const running = signal(false);
  const unknown = signal(false);
  const events = signal<string[]>([]);
  const log = (line: string) => events.set([line, ...events()].slice(0, 5));
  // The sweep of the indeterminate bars: still with reduced motion.
  const end = signal(false);
  const sweep = setInterval(() => {
    if (!reduceMotion()) end.set(!end());
  }, SWEEP);
  cleanup(() => clearInterval(sweep));
  let timer: ReturnType<typeof setInterval> | undefined;
  const stop = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
    running.set(false);
  };
  const start = () => {
    if (sent() >= 100) sent.set(0);
    running.set(true);
    log(`Start at ${sent()}%`);
    timer = setInterval(() => {
      sent.set(Math.min(100, sent() + 5));
      if (sent() >= 100) {
        stop();
        log("Done: value 100");
      }
    }, 200);
  };
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 14 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>progress</Text>
      <Text style={{ color: ui().muted }}>{desktop ? "Click Start, or Tab to it and press Space" : "Tap Start to send the photos"}</Text>

      <Split
        gap={14}
        inspector={
          <>
            <Panel title="value · indeterminate">
              <Text style={{ fontSize: 13, color: ui().text, fontVariant: ["tabular-nums"] }}>
                {unknown() ? "indeterminate (no value for screen readers)" : `value ${sent()} of 100`}
              </Text>
              <View class={desktop ? "flex-row self-start gap-2" : "flex-row gap-2"}>
                <Action label={running() ? "Pause" : sent() >= 100 ? "Again" : "Start"} disabled={unknown()} onPress={() => (running() ? (stop(), log(`Pause at ${sent()}%`)) : start())} />
                <Action
                  label="Reset"
                  disabled={sent() === 0}
                  onPress={() => {
                    stop();
                    sent.set(0);
                    log("Reset: value 0");
                  }}
                />
              </View>
              <Switch
                checked={unknown()}
                onChange={({ checked }) => {
                  unknown.set(checked);
                  if (checked) stop();
                  log(checked ? "Size unknown: indeterminate" : "Size known: value " + sent());
                }}
                {...listRow({ first: true, style: { flexDirection: "row-reverse" } })}
                {...switchLook(() => unknown())}
              >
                <RowLabel label="Size unknown" />
              </Switch>
            </Panel>
            <Log title="events" lines={events()} minHeight={92} />
          </>
        }
      >
        <View style={sectionSurface({ padded: true })}>
          <Transfer
            first
            label="Sending 24 photos"
            detail={unknown() ? "Counting what to send" : sent() >= 100 ? "Sent" : `${Math.round((sent() / 100) * 24)} of 24`}
            value={sent()}
            indeterminate={unknown()}
            end={end()}
          />
          <Transfer label="Preparing the backup" detail="Its size is not known yet" value={0} indeterminate end={end()} />
          <Transfer label="Library" detail="212 GB of 256 GB" value={83} end={end()} />
        </View>
      </Split>
    </View>
  );
}

mount(App);
