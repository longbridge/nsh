import { computed, signal, Text, View } from "nsh";
import { Checkbox, Radio, RadioGroup, Switch } from "nsh/base";
import { Icon } from "../icons";
import { Badge, Button, Caption, Card, colors, DemoPage, finish, mono, switchLook } from "../ui";

// `nsh/base`'s controls own presses, keys, the disabled state and what
// screen readers say; every value is the app's, and so is the look: the
// gallery's own here.

/** A control's row: the control and its label, the whole row pressable. */
const row = (first: boolean) => ({
  minHeight: 56,
  flexDirection: "row" as const,
  alignItems: "center" as const,
  gap: 12,
  paddingVertical: 8,
  borderTopWidth: first ? 0 : 1,
  borderTopColor: colors.line,
});
const pressed = { opacity: 0.7 };
const dimmed = { opacity: 0.4 };

/** The fill of a box or ring that is on, and of one that is off. */
const fill = (on: boolean) =>
  on
    ? { backgroundColor: colors.accentFill, borderColor: colors.accentFill, boxShadow: finish.chip.boxShadow }
    : { backgroundColor: colors.field, borderColor: colors.fieldBorder, boxShadow: finish.field.boxShadow };

type Mark = "check" | "dash" | null;

/** A checkbox's box: empty, or citrus with a check or a dash. */
const box = (mark: Mark) => ({
  width: 22,
  height: 22,
  borderRadius: 7,
  alignItems: "center" as const,
  justifyContent: "center" as const,
  borderWidth: 1,
  ...fill(mark !== null),
});

/** What a checkbox's box shows. */
function BoxMark(props: { mark: Mark }) {
  return (
    <>
      <If when={props.mark === "check"}>
        <Icon name="check" size={16} tint={colors.onAccent} />
      </If>
      <If when={props.mark === "dash"}>
        <View style={{ width: 10, height: 2, borderRadius: 1, backgroundColor: colors.onAccent }} />
      </If>
    </>
  );
}

/** A radio's ring: empty, or citrus with a dot. */
function Ring(props: { on: boolean }) {
  return (
    <View style={{ width: 22, height: 22, borderRadius: 11, alignItems: "center", justifyContent: "center", borderWidth: 1, ...fill(props.on) }}>
      <If when={props.on}>
        <View style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: colors.onAccent }} />
      </If>
    </View>
  );
}

/** A label, and a quieter line under it. */
function Label(props: { label: string; detail?: string }) {
  return (
    <View style={{ flex: 1, gap: 2 }}>
      <Text style={{ fontSize: 15, color: colors.text }}>{props.label}</Text>
      <If when={props.detail}>
        <Text style={{ fontSize: 13, color: colors.muted }}>{props.detail}</Text>
      </If>
    </View>
  );
}

const KINDS: { name: string; count: number }[] = [
  { name: "Photos", count: 1204 },
  { name: "Videos", count: 86 },
  { name: "Screenshots", count: 312 },
  { name: "Live Photos", count: 48 },
];

const PLANS: { name: string; detail: string; price: string; disabled?: boolean }[] = [
  { name: "Free", detail: "One device, 5 GB", price: "$0" },
  { name: "Plus", detail: "Three devices, 200 GB", price: "$3 / mo" },
  { name: "Team", detail: "A shared library, 2 TB", price: "$12 / mo" },
  { name: "Enterprise", detail: "Set up by your organization", price: "—", disabled: true },
];

const count = (n: number) => n.toLocaleString("en-US");

/**
 * Controls: `nsh/base`'s `Switch`, `Checkbox`, `Radio` and `RadioGroup` in
 * the settings, filters and pickers apps use them for.
 */
export function Controls() {
  const last = signal("—");

  function Settings() {
    const notifications = signal(true);
    const badges = signal(true);
    const haptics = signal(false);
    const lowData = signal(false);
    function Setting(props: { label: string; detail: string; on: boolean; set: (on: boolean) => void; disabled?: boolean; first?: boolean }) {
      const look = switchLook(() => props.on);
      return (
        <Switch
          checked={props.on}
          disabled={props.disabled}
          onChange={(on) => {
            props.set(on);
            last.set(`${props.label}: ${on ? "on" : "off"}`);
          }}
          // The track at the trailing edge, after the label.
          style={{ ...row(!!props.first), flexDirection: "row-reverse" }}
          activeStyle={pressed}
          disabledStyle={dimmed}
          trackStyle={look.trackStyle}
          thumbStyle={look.thumbStyle}
        >
          <Label label={props.label} detail={props.detail} />
        </Switch>
      );
    }
    return (
      <Card icon="controls" title="Switch" subtitle="Settings that take effect at once">
        <View>
          <Setting
            first
            label="Notifications"
            detail={notifications() ? "Banners, sounds and badges" : "Off"}
            on={notifications()}
            set={(on) => notifications.set(on)}
          />
          <Setting
            label="Badges"
            detail={notifications() ? "The unread count on the icon" : "Turn on notifications first"}
            on={badges()}
            disabled={!notifications()}
            set={(on) => badges.set(on)}
          />
          <Setting label="Haptics" detail="A tap under the finger as values change" on={haptics()} set={(on) => haptics.set(on)} />
          <Setting label="Low data mode" detail={lowData() ? "Images load at a lower quality" : "Images load in full"} on={lowData()} set={(on) => lowData.set(on)} />
        </View>
      </Card>
    );
  }

  function Filter() {
    const shown = signal<string[]>(["Photos", "Videos", "Live Photos"]);
    const has = (name: string) => shown().includes(name);
    const total = computed(() => KINDS.filter((k) => has(k.name)).reduce((n, k) => n + k.count, 0));
    const all = () => shown().length === KINDS.length;
    const mark = (): Mark => (all() ? "check" : shown().length > 0 ? "dash" : null);
    return (
      <Card icon="controls" title="Checkbox" subtitle="A filter: “Select all” shows a partial selection" right={<Badge label={`${shown().length} of ${KINDS.length}`} />}>
        <View>
          <Checkbox
            checked={all()}
            indeterminate={mark() === "dash"}
            onChange={(on) => {
              shown.set(on ? KINDS.map((k) => k.name) : []);
              last.set(`Select all: ${on}`);
            }}
            style={row(true)}
            activeStyle={pressed}
            indicatorStyle={box(mark())}
            indicator={<BoxMark mark={mark()} />}
          >
            <Text style={{ flex: 1, fontSize: 15, fontWeight: "600", color: colors.text }}>Select all</Text>
          </Checkbox>
          <Each of={KINDS}>
            {(kind) => (
              <Checkbox
                checked={has(kind().name)}
                onChange={(on) => {
                  shown.set(on ? KINDS.map((k) => k.name).filter((n) => n === kind().name || has(n)) : shown().filter((n) => n !== kind().name));
                  last.set(`${kind().name}: ${on}`);
                }}
                // One level under "Select all": the boxes line up with its label.
                style={{ ...row(false), paddingStart: 34 }}
                activeStyle={pressed}
                indicatorStyle={box(has(kind().name) ? "check" : null)}
                indicator={<BoxMark mark={has(kind().name) ? "check" : null} />}
              >
                <Text style={{ flex: 1, fontSize: 15, color: colors.text }}>{kind().name}</Text>
                <Text style={{ fontSize: 13, color: colors.faint, fontVariant: ["tabular-nums"] }}>{count(kind().count)}</Text>
              </Checkbox>
            )}
          </Each>
        </View>
        <Caption label={shown().length ? `Showing ${count(total())} items` : "Nothing to show"} />
      </Card>
    );
  }

  function Plans() {
    const plan = signal("Plus");
    const chosen = () => PLANS.find((p) => p.name === plan())!;
    return (
      <Card icon="controls" title="Radio" subtitle="A plan picker: one of a small set, all in view">
        <RadioGroup>
          <Each of={PLANS}>
            {(p, i) => (
              <Radio
                checked={plan() === p().name}
                disabled={p().disabled}
                onChange={() => {
                  plan.set(p().name);
                  last.set(`Plan: ${p().name}`);
                }}
                style={row(i() === 0)}
                activeStyle={pressed}
                disabledStyle={dimmed}
              >
                <Ring on={plan() === p().name} />
                <Label label={p().name} detail={p().detail} />
                <Text style={{ fontSize: 15, color: plan() === p().name ? colors.text : colors.muted, fontVariant: ["tabular-nums"] }}>{p().price}</Text>
              </Radio>
            )}
          </Each>
        </RadioGroup>
        <Button label={`Continue with ${plan()}`} onPress={() => last.set(`Continue: ${chosen().name}, ${chosen().price}`)} />
      </Card>
    );
  }

  function Last() {
    return (
      <Card icon="system" title="Last event">
        <Text style={{ fontSize: 13, color: colors.text, fontFamily: mono }}>{last()}</Text>
      </Card>
    );
  }

  return <DemoPage cards={[Settings, Filter, Plans, Last]} />;
}
