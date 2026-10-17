import { cleanup, signal, Text, View } from "nsh";
import { createNativeValue, Easing } from "nsh/animated";
import { Toast } from "nsh/android";
import { colorScheme, fontScale, windowSize } from "nsh/device";
import { keymap } from "nsh/keymap";
import { ActionSheet, Alert, keyboard, safeArea, StatusBar } from "nsh/ui/mobile";
import { Icon, type IconName } from "../icons";
import {
  Appear,
  Badge,
  Button,
  Caption,
  Card,
  colors,
  DemoPage,
  Figure,
  finish,
  IconTile,
  isDark,
  Inset,
  Legend,
  Meter,
  mono,
  Press,
  Row,
  Segmented,
  Stat,
  tints,
  Toggle,
  type Tone,
  toneColor,
} from "../ui";

/** The window drawn to scale, with the safe area insets shaded. */
function SafeAreaDiagram() {
  const width = 76;
  const scale = () => width / Math.max(1, windowSize().width);
  const height = () => Math.round(windowSize().height * scale());
  const band = (pt: number) => Math.max(0, Math.round(pt * scale()));
  return (
    <View
      style={{
        width,
        height: height(),
        borderRadius: 14,
        borderCurve: "continuous",
        borderWidth: 1.5,
        borderColor: colors.border,
        backgroundColor: colors.surface,
        backgroundImage: finish.card.backgroundImage,
        boxShadow: isDark() ? "inset 0 1px 0 rgba(255,255,255,0.06)" : "inset 0 1px 0 #ffffff",
        overflow: "hidden",
      }}
    >
      <View style={{ height: band(safeArea().top), backgroundColor: tints.info }} />
      <View style={{ flex: 1, margin: 6, borderRadius: 6, borderWidth: 1, borderColor: colors.line, borderStyle: "dashed" }} />
      <View style={{ height: band(keyboard().height), backgroundColor: tints.warn }} />
      <View style={{ height: band(safeArea().bottom), backgroundColor: tints.info }} />
    </View>
  );
}

function Environment() {
  const insets = () => [
    { label: "Top", value: safeArea().top },
    { label: "Right", value: safeArea().right },
    { label: "Bottom", value: safeArea().bottom },
    { label: "Left", value: safeArea().left },
  ];
  return (
    <Card icon="system" title="Environment" subtitle="Updates when the window, insets or keyboard change" right={<Badge label="Live" tone="up" />}>
      <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
        <View style={{ flex: 1, gap: 12 }}>
          <View style={{ gap: 2 }}>
            <Text style={{ fontSize: 12, color: colors.muted }}>Window</Text>
            <Figure value={`${Math.round(windowSize().width)} × ${Math.round(windowSize().height)}`} unit="pt" />
          </View>
          <View style={{ flexDirection: "row" }}>
            <Stat label="Keyboard" value={`${Math.round(keyboard().height)} pt`} tone={keyboard().height > 0 ? "warn" : undefined} width="50%" />
            <Stat label="Font scale" value={`${fontScale().toFixed(2)}×`} width="50%" />
          </View>
        </View>
        <SafeAreaDiagram />
      </View>
      <View style={{ height: 1, backgroundColor: colors.line }} />
      <View style={{ gap: 4 }}>
        <Caption label="Safe area insets" note="pt" />
        <View style={{ flexDirection: "row" }}>
          <Each of={insets()}>{(inset) => <Stat label={inset().label} value={String(Math.round(inset().value))} width="25%" />}</Each>
        </View>
      </View>
      <View style={{ height: 1, backgroundColor: colors.line }} />
      <View style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
        <Legend color={colors.info} label="Safe area" />
        <Legend color={colors.warn} label="Keyboard" />
        <View style={{ flex: 1 }} />
        <Badge label={colorScheme() === "dark" ? "Dark scheme" : "Light scheme"} tone={colorScheme() === "dark" ? "info" : "warn"} outline />
      </View>
    </Card>
  );
}

type BarStyle = "light-content" | "dark-content" | "default";

function StatusBarCard() {
  // Until one is picked, the style follows the appearance (light text on the dark page).
  const picked = signal<BarStyle | null>(null);
  const barStyle = () => picked() ?? (isDark() ? "light-content" : "dark-content");
  const barHidden = signal(false);
  return (
    <Card icon="mobile" title="Status bar" subtitle="StatusBar: style and visibility, animated">
      <StatusBar barStyle={barStyle()} hidden={barHidden()} animated />
      <Segmented
        options={[
          { value: "light-content", label: "Light" },
          { value: "dark-content", label: "Dark" },
          { value: "default", label: "Default" },
        ]}
        value={barStyle()}
        onChange={(v) => picked.set(v)}
      />
      <Row>
        <View style={{ flex: 1, gap: 2 }}>
          <Text style={{ fontSize: 15, color: colors.text }}>Hidden</Text>
          <Text style={{ fontSize: 12, color: colors.muted, fontFamily: mono }}>{`barStyle: "${barStyle()}"`}</Text>
        </View>
        <Toggle value={barHidden()} onChange={(v) => barHidden.set(v)} label="Hide the status bar" />
      </Row>
    </Card>
  );
}

/** A tappable row with an icon tile, a title, a detail and a chevron. */
function ActionRow(props: { first?: boolean; icon: IconName; tint: string; title: string; detail: string; onPress: () => void }) {
  return (
    <Press onPress={props.onPress} scale={0.98} accessibilityLabel={props.title}>
      <Row first={props.first}>
        <IconTile name={props.icon} tint={props.tint} size={34} />
        <View style={{ flex: 1, gap: 1, paddingVertical: 8 }}>
          <Text style={{ fontSize: 15, color: colors.text }}>{props.title}</Text>
          <Text style={{ fontSize: 12, color: colors.muted }}>{props.detail}</Text>
        </View>
        <Icon name="chevron" size={14} tint={colors.faint} />
      </Row>
    </Press>
  );
}

function Dialogs() {
  const answer = signal<{ text: string; tone: Tone } | null>(null);
  // The answer pops in each time it changes.
  const pop = createNativeValue(1);
  const show = (text: string, tone: Tone = "neutral") => {
    answer.set({ text, tone });
    pop.set(0.85);
    pop.spring(1, { stiffness: 380, damping: 18 });
  };
  return (
    <Card icon="bell" title="Alerts and sheets" subtitle="Alert · ActionSheet · Toast" flush>
      <Inset gap={0} padding={12}>
        <ActionRow
          first
          icon="check"
          tint={colors.up}
          title="Alert"
          detail="One button"
          onPress={async () => {
            await Alert.alert("Saved", "Your changes were saved.");
            show("OK", "up");
          }}
        />
        <ActionRow
          icon="bell"
          tint={colors.danger}
          title="Confirm"
          detail="Cancel and destructive buttons"
          onPress={async () => {
            const i = await Alert.alert(
              "Delete the photo?",
              "This cannot be undone.",
              [
                { text: "Cancel", style: "cancel" },
                { text: "Delete", style: "destructive" },
              ],
              { cancelable: true },
            );
            const text = ["Cancel", "Delete"][i] ?? "dismissed";
            show(text, text === "Delete" ? "danger" : "neutral");
          }}
        />
        <ActionRow
          icon="star"
          tint={colors.info}
          title="Three buttons"
          detail="Resolves to the index pressed"
          onPress={async () => {
            const buttons = ["Later", "Never", "Install"];
            const i = await Alert.alert("Update available", "Install it now?", [{ text: "Later" }, { text: "Never", style: "cancel" }, { text: "Install" }]);
            show(buttons[i] ?? "dismissed", buttons[i] === "Install" ? "info" : "neutral");
          }}
        />
        <ActionRow
          icon="lists"
          tint={colors.accent}
          title="Action sheet"
          detail="Title, cancel and destructive options"
          onPress={async () => {
            const options = ["Cancel", "Copy link", "Save", "Delete"];
            const i = await ActionSheet.show({ title: "Share the photo", options, cancelButtonIndex: 0, destructiveButtonIndexes: [3] });
            const text = options[i] ?? "dismissed";
            show(text, text === "Delete" ? "danger" : "accent");
          }}
        />
        <ActionRow icon="spark" tint={colors.warn} title="Toast" detail="Android only" onPress={() => Toast.show("Hello from a toast", "long")} />
      </Inset>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 11, paddingTop: 12, paddingBottom: 11, minHeight: 44 }}>
        <Text style={{ flex: 1, fontSize: 13, color: colors.muted }}>Answer</Text>
        <If when={answer()} else={<Text style={{ fontSize: 13, color: colors.faint }}>None yet</Text>}>
          <View style={{ transform: [{ scale: pop }] }}>
            <Badge label={answer()!.text} tone={answer()!.tone} />
          </View>
        </If>
      </View>
    </Card>
  );
}

function FramesAndTimers() {
  const fps = signal(0);
  const running = signal(false);
  const ticks = signal(0);
  // The beat dot flashes once per tick; the fade runs natively.
  const beat = createNativeValue(0);

  // Frames per second, counted with requestAnimationFrame.
  let frame = 0;
  let frames = 0;
  let since = 0;
  const loop = (time: number) => {
    frames++;
    if (time - since >= 1000) {
      fps.set(Math.round((frames * 1000) / (time - since)));
      frames = 0;
      since = time;
    }
    frame = requestAnimationFrame(loop);
  };
  const toggle = () => {
    if (running()) cancelAnimationFrame(frame);
    else {
      frames = 0;
      since = performance.now();
      frame = requestAnimationFrame(loop);
    }
    running.set(!running());
  };
  const timer = setInterval(() => {
    ticks.set(ticks() + 1);
    beat.set(1);
    beat.timing(0, { duration: 700, easing: Easing.out(Easing.quad) });
  }, 1000);
  cleanup(() => {
    clearInterval(timer);
    cancelAnimationFrame(frame);
  });

  return (
    <Card icon="animation" title="Frames and timers" subtitle="requestAnimationFrame · setInterval">
      <View style={{ flexDirection: "row", gap: 12 }}>
        <View style={{ flex: 1, gap: 2 }}>
          <Text style={{ fontSize: 12, color: colors.muted }}>Frame rate</Text>
          <Figure value={running() ? String(fps()) : "–"} unit="fps" note={running() ? "live" : "stopped"} tone={running() ? "up" : "neutral"} />
        </View>
        <View style={{ width: 1, backgroundColor: colors.line }} />
        <View style={{ flex: 1, gap: 2 }}>
          <View style={{ flexDirection: "row", alignItems: "center", gap: 6 }}>
            <Text style={{ fontSize: 12, color: colors.muted }}>Interval ticks</Text>
            <View
              style={{
                width: 7,
                height: 7,
                borderRadius: 3.5,
                backgroundColor: colors.accent,
                opacity: beat.interpolate({ inputRange: [0, 1], outputRange: [0.25, 1] }),
                transform: [{ scale: beat.interpolate({ inputRange: [0, 1], outputRange: [1, 1.6] }) }],
              }}
            />
          </View>
          <Figure value={String(ticks())} note="every 1 s" tone="accent" />
        </View>
      </View>
      <View style={{ gap: 8 }}>
        <Caption label="Share of 120 fps" note={running() ? `${Math.round((fps() / 120) * 100)}%` : "–"} />
        <Meter value={running() ? fps() / 120 : 0} tone="up" marker={0.5} />
      </View>
      <Button label={running() ? "Stop counting frames" : "Count frames"} icon={running() ? "pause" : "play"} onPress={toggle} />
    </Card>
  );
}

/** App-level shortcuts: bound while the demo shows (the component's own). */
function Shortcuts() {
  const count = signal(0);
  const on = signal(false);
  const last = signal("");
  keymap.bind({
    "mod+k": (k) => {
      count.set(count() + 1);
      last.set(k);
    },
    "mod+shift+l": (k) => {
      on.set(!on());
      last.set(k);
    },
  });
  keymap.bind("mod+backspace", (k) => {
    count.set(0);
    last.set(k);
  }, { when: "notEditing" });
  const keys: [string, string][] = [
    [keymap.label("mod+k"), "Counts"],
    [keymap.label("mod+shift+l"), "Switches the light"],
    [keymap.label("mod+backspace"), "Resets (not while typing)"],
  ];
  return (
    <Card icon="bell" title="Keyboard shortcuts" subtitle="nsh/keymap · desktop">
      <View>
        <Each of={keys}>
          {(key, i) => (
            <Row first={i() === 0}>
              <Text style={{ width: 120, fontSize: 14, color: colors.accent, fontFamily: mono }}>{key()[0]}</Text>
              <Text style={{ flex: 1, fontSize: 15, color: colors.text }}>{key()[1]}</Text>
            </Row>
          )}
        </Each>
      </View>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Badge label={on() ? "Light on" : "Light off"} tone={on() ? "up" : "neutral"} />
        <View style={{ flex: 1 }}>
          <Caption label={`Counted ${count()}`} note={last() || "–"} />
        </View>
      </View>
    </Card>
  );
}

type Post = { id: number; title: string };

function Network() {
  const posts = signal<Post[]>([]);
  const status = signal<{ text: string; tone: Tone } | null>(null);
  const loading = signal(false);
  const load = async () => {
    loading.set(true);
    status.set({ text: "loading…", tone: "neutral" });
    const started = performance.now();
    try {
      const response = await fetch("https://jsonplaceholder.typicode.com/posts?_limit=3");
      posts.set(await response.json());
      status.set({ text: `HTTP ${response.status} · ${Math.round(performance.now() - started)} ms`, tone: response.ok ? "up" : "danger" });
    } catch (e) {
      status.set({ text: String(e), tone: "danger" });
    }
    loading.set(false);
  };
  return (
    <Card icon="search" title="Network and logs" subtitle="fetch() · console.log">
      <If when={posts().length > 0} else={<Caption label="Three posts from jsonplaceholder.typicode.com" />}>
        <View>
          <Each of={posts()}>
            {(post, i) => (
              <Appear index={i()}>
                <Row first={i() === 0}>
                  <Text style={{ width: 28, fontSize: 13, color: colors.accent, fontVariant: ["tabular-nums"] }}>{`#${post().id}`}</Text>
                  <Text numberOfLines={1} style={{ flex: 1, fontSize: 15, color: colors.text }}>
                    {post().title}
                  </Text>
                </Row>
              </Appear>
            )}
          </Each>
        </View>
      </If>
      <If when={status()}>
        <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
          <View style={{ width: 7, height: 7, borderRadius: 3.5, backgroundColor: toneColor(status()!.tone) }} />
          <Text numberOfLines={2} style={{ flex: 1, fontSize: 13, color: toneColor(status()!.tone), fontVariant: ["tabular-nums"] }}>
            {status()!.text}
          </Text>
        </View>
      </If>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <View style={{ flex: 1 }}>
          <Button label={loading() ? "Loading…" : "fetch()"} icon="arrow-down" disabled={loading()} onPress={load} />
        </View>
        <View style={{ flex: 1 }}>
          <Button
            label="console.log"
            variant="secondary"
            onPress={() => console.log("hello from the gallery", { at: new Date().toISOString() })}
          />
        </View>
      </View>
      <Text style={{ fontSize: 12, color: colors.faint }}>console.log shows in the terminal of nsh run.</Text>
    </Card>
  );
}

export function System() {
  const sections = [Environment, StatusBarCard, Dialogs, FramesAndTimers, Shortcuts, Network];
  return <DemoPage cards={sections} />;
}
