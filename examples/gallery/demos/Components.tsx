import { Input, type InputProps, type JSX, signal, Text, View, type YNode } from "nsh";
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";
import { LayoutAnimation } from "nsh/animated";
import { PixelRatio, Platform } from "nsh/device";
import { InputAccessoryView } from "nsh/ios";
import { Button, Card, KeyboardAvoidingView, Modal, SafeAreaView, Screen } from "nsh/ui/mobile";
import { Icon, type IconName } from "../icons";
import { nav } from "../nav";
import {
  Appear,
  Badge,
  Button as UiButton,
  Caption,
  Card as UiCard,
  colors,
  DemoPage,
  finish,
  header,
  hint,
  IconTile,
  Inset,
  isDark,
  mono,
  Press,
  radius,
  Row,
  Segmented,
  tints,
  Toggle,
  type Tone,
  toneColor,
  toneTint,
} from "../ui";

const hairline = 1 / PixelRatio.get();

/** A dark input, 48 pt tall. */
function Field(props: InputProps) {
  const focused = signal(false);
  return (
    <Input
      placeholderTextColor={colors.faint}
      selectionColor={colors.accent}
      onFocus={() => focused.set(true)}
      onBlur={() => focused.set(false)}
      {...props}
      style={{
        height: 48,
        paddingHorizontal: 14,
        fontSize: 15,
        color: colors.text,
        backgroundColor: colors.inset,
        borderWidth: 1,
        borderColor: focused() ? colors.accent : colors.border,
        borderRadius: radius.control,
        ...finish.inset,
      }}
    />
  );
}

/** A cell of a row of equal widths. */
function Cell(props: { children: JSX.Element }) {
  return <View style={{ flex: 1 }}>{props.children}</View>;
}

/** A row that pushes a screen. */
function LinkRow(props: { icon: IconName; title: string; note: string; onPress: () => void }) {
  return (
    <Press onPress={props.onPress} scale={0.98} accessibilityLabel={props.title}>
      <View style={{ minHeight: 60, flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 10, paddingVertical: 8 }}>
        <IconTile name={props.icon} size={34} />
        <View style={{ flex: 1, gap: 2 }}>
          <Text style={{ fontSize: 15, fontWeight: "500", color: colors.text }}>{props.title}</Text>
          <Text numberOfLines={1} style={{ fontSize: 12, color: colors.faint, fontFamily: mono }}>
            {props.note}
          </Text>
        </View>
        <Icon name="chevron" size={14} tint={colors.faint} />
      </View>
    </Press>
  );
}

const presets = [
  { value: "spring", label: "Spring" },
  { value: "easeInEaseOut", label: "Ease" },
  { value: "linear", label: "Linear" },
] as const;

const tileTones: Tone[] = ["accent", "info", "warn", "up", "danger"];

function LayoutAnimations() {
  const items = signal([1, 2, 3]);
  const preset = signal<(typeof presets)[number]["value"]>("spring");
  let next = 4;
  const animate = (change: () => void) => {
    LayoutAnimation.configureNext(LayoutAnimation.Presets[preset()]);
    change();
  };
  return (
    <UiCard icon="animation" title="Layout animation" subtitle="configureNext, then change the tree" right={<Badge label={`${items().length}`} />}>
      <Segmented options={presets} value={preset()} onChange={(p) => preset.set(p)} size="small" />
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 8, minHeight: 56 }}>
        <Each of={items()}>
          {(n) => (
            <View
              style={{
                width: 56,
                height: 56,
                borderRadius: 14,
                borderCurve: "continuous",
                backgroundColor: toneTint(tileTones[n() % tileTones.length]),
                boxShadow: isDark() ? "inset 0 1px 0 rgba(255,255,255,0.06)" : "inset 0 1px 0 #ffffff",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Text style={{ fontSize: 17, fontWeight: "600", fontVariant: ["tabular-nums"], color: toneColor(tileTones[n() % tileTones.length]) }}>{n()}</Text>
            </View>
          )}
        </Each>
      </View>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Cell><UiButton label="Add" icon="plus" onPress={() => animate(() => items.set([next++, ...items()]))} /></Cell>
        <Cell><UiButton label="Shuffle" variant="secondary" onPress={() => animate(() => items.set([...items()].sort(() => Math.random() - 0.5)))} /></Cell>
        <Cell><UiButton label="Remove" variant="secondary" disabled={items().length === 0} onPress={() => animate(() => items.set(items().slice(1)))} /></Cell>
      </View>
    </UiCard>
  );
}

function PressableSheet() {
  const sheet = signal(false);
  return (
    <UiCard icon="components" title="Sheet" subtitle="A Tailwind active: variant opens it">
      <View
        onPress={() => sheet.set(true)}
        accessibilityRole="button"
        style={{ boxShadow: finish.accent.boxShadow }}
        class="h-12 flex-row items-center justify-center gap-2 rounded-[12px] bg-[#c8f26d] transition active:scale-[0.97] active:bg-[#a9d14f]"
      >
        <Text style={{ fontSize: 15, fontWeight: "600", color: colors.onAccent }}>Open a sheet</Text>
      </View>
      <Caption label="Scales and darkens while pressed" note="active:scale-[0.97]" />
      <Modal
        visible={sheet()}
        ios={{ presentationStyle: "pageSheet" }}
        desktop={{ presentationStyle: "pageSheet" }}
        web={{ presentationStyle: "pageSheet" }}
        animationType="slide"
        onRequestClose={() => sheet.set(false)}
      >
        <SafeAreaView style={{ flex: 1, backgroundColor: colors.background }}>
          <View style={{ flex: 1, padding: 16, gap: 16 }}>
            <View style={{ gap: 8, paddingHorizontal: 4, paddingTop: 8 }}>
              <View style={{ flexDirection: "row" }}>
                <Badge label="pageSheet" tone="accent" />
              </View>
              <Text style={{ fontSize: 28, fontWeight: "700", letterSpacing: -0.5, color: colors.text }}>A sheet</Text>
              <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>
                {hint("Swipe down (iOS) or press back (Android) to close.", "Press Escape or click outside the card to close.")}
              </Text>
            </View>
            <View style={{ flex: 1 }} />
            <UiButton label="Close" onPress={() => sheet.set(false)} />
          </View>
        </SafeAreaView>
      </Modal>
    </UiCard>
  );
}

/** A tile to press, with the prop it shows. */
function Tile(props: { title: string; note: string; children?: JSX.Element }) {
  return (
    <View style={{ minHeight: 60, flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 12, paddingVertical: 8 }}>
      <View style={{ flex: 1, gap: 2 }}>
        <Text style={{ fontSize: 15, color: colors.text }}>{props.title}</Text>
        <Text style={{ fontSize: 12, color: colors.faint, fontFamily: mono }}>{props.note}</Text>
      </View>
      {props.children}
    </View>
  );
}

const tile = () =>
  ({
    borderRadius: radius.control,
    borderCurve: "continuous",
    borderWidth: 1,
    borderColor: colors.border,
    backgroundColor: colors.surface,
    // The raised highlight without its gradient, so an underlay color shows.
    boxShadow: finish.raised.boxShadow,
  }) as const;

/** Press feedback is native: opacity, an underlay, classes; `disabled` takes no presses. */
function PressFeedback() {
  const last = signal("None yet");
  const disabled = signal(false);
  return (
    <UiCard icon="gestures" title="Press feedback" subtitle={hint("Drawn natively while the finger is down", "Drawn natively while the button is down")}>
      <View style={tile()} activeOpacity={0.4} onPress={() => last.set("Fading tile")}>
        <Tile title="Fades while pressed" note="activeOpacity={0.4}" />
      </View>
      <View style={tile()} underlayColor={colors.raised} onPress={() => last.set("Underlay tile")}>
        <Tile title="Lights up while pressed" note="underlayColor" />
      </View>
      <View style={tile()} disabled={disabled()} underlayColor={tints.danger} class="disabled:opacity-40" onPress={() => last.set("Enabled tile")}>
        <Tile title={disabled() ? "Takes no presses" : "Enabled"} note="disabled + disabled:opacity-40">
          <Badge label={disabled() ? "Off" : "On"} tone={disabled() ? "neutral" : "up"} />
        </Tile>
      </View>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
        <View style={{ flex: 1 }}>
          <Caption label="Last pressed" note={last()} />
        </View>
        <Toggle value={disabled()} onChange={(v) => disabled.set(v)} label="Disable the third tile" />
      </View>
    </UiCard>
  );
}

function Accessory() {
  const text = signal("");
  let input: YNode | undefined;
  return (
    <UiCard icon="text" title="Input accessory" subtitle="A bar over the iOS keyboard" right={<Badge label="nsh/ios" />}>
      <Field
        ref={(node) => (input = node)}
        value={text()}
        onChangeText={(v) => text.set(v)}
        placeholder="Focus me"
        ios={{ inputAccessoryViewID: "counter" }}
      />
      <Caption
        label={Platform.isDesktop ? "Desktops have no input accessories" : Platform.OS === "android" ? "Android has no input accessories" : "The bar counts what you type"} note='inputAccessoryViewID="counter"' />
      <InputAccessoryView nativeID="counter" style={{ backgroundColor: colors.surface }}>
        <View
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 8,
            paddingHorizontal: 16,
            paddingVertical: 8,
            borderTopWidth: hairline,
            borderTopColor: colors.border,
          }}
        >
          <Text style={{ flex: 1, fontSize: 13, color: colors.muted, fontVariant: ["tabular-nums"] }}>{`${text().length} characters`}</Text>
          <UiButton label="Clear" variant="secondary" onPress={() => text.set("")} />
          <UiButton label="Done" onPress={() => input?.blur()} />
        </View>
      </InputAccessoryView>
    </UiCard>
  );
}

const behaviors = [
  { value: "padding", label: "padding" },
  { value: "height", label: "height" },
  { value: "position", label: "position" },
] as const;

const behaviorNote = {
  padding: "The bottom pads: the tinted background stays under the keyboard.",
  height: "The view shrinks: its background ends at the keyboard.",
  position: "The inner content view moves up as a whole.",
} as const;

function AvoidingPage() {
  const behavior = signal<"padding" | "height" | "position">("padding");
  return (
    <Screen title="KeyboardAvoidingView" {...header}>
      <KeyboardAvoidingView behavior={behavior()} style={{ flex: 1, backgroundColor: tints.info }} contentContainerStyle={{ flex: 1 }}>
        <View style={{ flex: 1, padding: 16, gap: 16 }}>
          <Appear>
            <UiCard icon="form" title={`behavior="${behavior()}"`} subtitle="Focus the field at the bottom">
              <Segmented options={behaviors} value={behavior()} onChange={(b) => behavior.set(b)} />
              <Text style={{ fontSize: 14, lineHeight: 20, color: colors.muted }}>{behaviorNote[behavior()]}</Text>
            </UiCard>
          </Appear>
          <View style={{ flex: 1 }} />
          <Field placeholder="Type here" />
        </View>
      </KeyboardAvoidingView>
    </Screen>
  );
}

function ModalOptions() {
  const visible = signal(false);
  const translucent = signal(false);
  return (
    <UiCard
      icon="system"
      title="Modal options"
      subtitle="statusBarTranslucent, orientations"
      right={<Toggle value={translucent()} onChange={(v) => translucent.set(v)} label="statusBarTranslucent" />}
    >
      <Text style={{ fontSize: 14, lineHeight: 20, color: colors.muted }}>
        Android: translucent content reaches under the status bar. iOS: this modal allows portrait and landscape.
      </Text>
      <Caption label="statusBarTranslucent" note={String(translucent())} />
      <UiButton label="Open the modal" variant="secondary" onPress={() => visible.set(true)} />
      <Modal
        visible={visible()}
        animationType="slide"
        android={{ statusBarTranslucent: translucent() }}
        ios={{ supportedOrientations: ["portrait", "landscape"] }}
        onRequestClose={() => visible.set(false)}
      >
        <View style={{ flex: 1, backgroundColor: isDark() ? "#16200d" : "#eef5dc" }}>
          <SafeAreaView style={{ flex: 1, padding: 16, gap: 12 }}>
            <View style={{ flexDirection: "row", paddingTop: 8 }}>
              <Badge label={translucent() ? "translucent" : "opaque bar"} tone="accent" />
            </View>
            <Text style={{ fontSize: 28, fontWeight: "700", letterSpacing: -0.5, color: colors.text }}>A modal</Text>
            <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>
              The olive background starts {translucent() ? "under" : "below"} the status bar on Android.
            </Text>
            <View style={{ flex: 1 }} />
            <UiButton label="Close" onPress={() => visible.set(false)} />
          </SafeAreaView>
        </View>
      </Modal>
    </UiCard>
  );
}

function PlatformInfo() {
  const rows = (): [string, string][] => [
    ["Platform.OS", Platform.OS],
    ["Platform.Version", String(Platform.Version)],
    ["isPad / isTV / isMacCatalyst", `${Platform.isPad} / ${Platform.isTV} / ${Platform.isMacCatalyst}`],
    ["Platform.select", Platform.select({ ios: "ios branch", android: "android branch" }) ?? ""],
    ["PixelRatio.get()", String(PixelRatio.get())],
    ["roundToNearestPixel(10.3)", String(PixelRatio.roundToNearestPixel(10.3))],
    ["screenReaderEnabled()", String(screenReaderEnabled())],
    ["reduceMotion()", String(reduceMotion())],
  ];
  const announced = signal(0);
  let target: YNode | undefined;
  return (
    <UiCard icon="spark" title="Platform" subtitle="Platform, PixelRatio, AccessibilityInfo" right={<Badge label={Platform.OS} tone="accent" />}>
      <View>
        <Each of={rows()}>
          {(row, i) => (
            <Row first={i() === 0}>
              <Text numberOfLines={1} style={{ flex: 1, fontSize: 12, color: colors.muted, fontFamily: mono }}>
                {row()[0]}
              </Text>
              <Text numberOfLines={1} style={{ fontSize: 14, color: colors.text, fontVariant: ["tabular-nums"] }}>
                {row()[1]}
              </Text>
            </Row>
          )}
        </Each>
      </View>
      <View style={{ height: hairline, backgroundColor: colors.accent }} />
      <Caption label="The line above is one device pixel tall" note={`${hairline.toFixed(3)} pt`} />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Cell><UiButton
          label="Announce"
          onPress={() => {
            announced.set(announced() + 1);
            AccessibilityInfo.announceForAccessibility(`Announcement ${announced()}`);
          }}
        /></Cell>
        <Cell><UiButton label="Focus the note" variant="secondary" onPress={() => target && AccessibilityInfo.setAccessibilityFocus(target)} /></Cell>
      </View>
      <Text ref={(node) => (target = node)} style={{ fontSize: 13, lineHeight: 18, color: colors.faint }}>
        With VoiceOver or TalkBack on, the buttons above speak and move the reader here.
      </Text>
    </UiCard>
  );
}

/** The components of nsh/ui/mobile (Rust, `nsh-ui-mobile`). */
function MobileComponents() {
  const presses = signal(0);
  return (
    <UiCard icon="mobile" title="nsh/ui/mobile" subtitle="Rust components, the app's theme" right={<Badge label={`${presses()} ${presses() === 1 ? hint("tap", "click") : hint("taps", "clicks")}`} tone={presses() ? "accent" : "neutral"} />}>
      <Button title="Primary" onPress={() => presses.set(presses() + 1)} />
      <View style={{ flexDirection: "row", gap: 8 }}>
        <View style={{ flex: 1 }}>
          <Button title="Secondary" variant="secondary" onPress={() => presses.set(presses() + 1)} />
        </View>
        <View style={{ flex: 1 }}>
          <Button title="Danger" variant="danger" onPress={() => presses.set(presses() + 1)} />
        </View>
      </View>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <View style={{ flex: 1 }}>
          <Button title="Plain" variant="plain" onPress={() => presses.set(presses() + 1)} />
        </View>
        <View style={{ flex: 1 }}>
          <Button title="Disabled" variant="secondary" disabled onPress={() => presses.set(presses() + 1)} />
        </View>
      </View>
      <Card
        header={<Text>Card with slots</Text>}
        footer={
          <View style={{ flexDirection: "row", gap: 8 }}>
            <View style={{ flex: 1 }}>
              <Button title="Reset" variant="secondary" onPress={() => presses.set(0)} />
            </View>
            <View style={{ flex: 1 }}>
              <Button title="Add one" onPress={() => presses.set(presses() + 1)} />
            </View>
          </View>
        }
      >
        <Text>The header, this body and the footer are JSX in a Rust component's slots and children.</Text>
      </Card>
      <Card variant="outlined">
        <Text>An outlined card, children only.</Text>
      </Card>
      <Caption label="Colors come from the app's theme" note="theme.json" />
    </UiCard>
  );
}

export function Components() {
  return (
    <DemoPage
      keyboardDismissMode="interactive"
      cards={[
        MobileComponents,
        PressFeedback,
        PressableSheet,
        LayoutAnimations,
        Accessory,
        () => (
          <UiCard title="Keyboard avoidance" subtitle="Content outside scroll views" flush>
            <Inset padding={0} gap={0}>
              <LinkRow icon="form" title="KeyboardAvoidingView" note="padding / height / position" onPress={() => nav.push({ name: "page", page: AvoidingPage })} />
            </Inset>
          </UiCard>
        ),
        ModalOptions,
        PlatformInfo,
      ]}
    />
  );
}
