import { cleanup, effect, Input, type JSX, Scroll, signal, Text, View } from "nsh";
import { createAnimatedValue } from "nsh/animated";
import { BackHandler } from "nsh/android";
import { Platform } from "nsh/device";
import { Alert, Modal, SafeAreaView, Screen, useSafeAreaInsets } from "nsh/ui/mobile";
import { Icon, type IconName } from "../icons";
import { nav } from "../nav";
import { Appear, Badge, Button, Card, colors, DemoPage, Figure, finish, header, hint, IconTile, Inset, isDark, mono, motion, Press, radius, Section, Stat, tints, Toggle } from "../ui";

const edges = ["top", "bottom", "left", "right"] as const;

/** The safe-area insets where it renders, one figure per edge. */
function Insets() {
  const insets = useSafeAreaInsets();
  return (
    <View style={{ flexDirection: "row" }}>
      <Each of={edges}>{(edge) => <Stat width="25%" label={edge()[0].toUpperCase() + edge().slice(1)} value={`${Math.round(insets()[edge()])} pt`} />}</Each>
    </View>
  );
}

/** A row that pushes a screen: an icon, a title, the API it shows. */
function LinkRow(props: { icon: IconName; title: string; note: string; first?: boolean; onPress: () => void }) {
  return (
    <Press onPress={props.onPress} scale={0.98} accessibilityLabel={props.title}>
      <View
        style={{
          minHeight: 60,
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          paddingHorizontal: 10,
          paddingVertical: 8,
          borderTopWidth: props.first ? 0 : 1,
          borderTopColor: colors.line,
        }}
      >
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

/** The body of a pushed screen: what it shows, then where its safe area is. */
function Page(props: { intro: string; children?: JSX.Element }) {
  return (
    <Scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <View style={{ padding: 16, paddingBottom: 40, gap: 16 }}>
        <Appear>
          <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted, paddingHorizontal: 4 }}>{props.intro}</Text>
        </Appear>
        <Appear index={1}>
          <Section title="Safe area" note="useSafeAreaInsets()">
            <Insets />
          </Section>
        </Appear>
        <Appear index={2}>
          <View style={{ gap: 16 }}>{props.children}</View>
        </Appear>
      </View>
    </Scroll>
  );
}

function LargeTitle() {
  return (
    <Screen title="Large title" {...header} ios={{ headerLargeTitle: true }}>
      <Page intro="iOS: the title shrinks into the bar as the content scrolls.">
        <Card flush>
          <Inset padding={0} gap={0}>
            <Each of={Array.from({ length: 20 }, (_, i) => i + 1)}>
              {(i) => (
                <View
                  style={{
                    height: 48,
                    flexDirection: "row",
                    alignItems: "center",
                    paddingHorizontal: 10,
                    borderTopWidth: i() === 1 ? 0 : 1,
                    borderTopColor: colors.line,
                  }}
                >
                  <Text style={{ flex: 1, fontSize: 15, color: colors.text }}>Row {i()}</Text>
                  <Text style={{ fontSize: 13, color: colors.faint, fontVariant: ["tabular-nums"] }}>{`${i() * 48} pt`}</Text>
                </View>
              )}
            </Each>
          </Inset>
        </Card>
      </Page>
    </Screen>
  );
}

function Transparent() {
  return (
    <Screen title="Transparent" headerTransparent headerTintColor={colors.text} headerTitleColor={colors.text}>
      <View style={{ flex: 1, backgroundColor: isDark() ? "#11170c" : "#eef3e2", overflow: "hidden", padding: 16, paddingBottom: 56, justifyContent: "flex-end", gap: 16 }}>
        <View style={{ position: "absolute", top: -90, right: -70, width: 320, height: 320, borderRadius: 160, backgroundColor: tints.accent }} />
        <View style={{ position: "absolute", top: 150, left: -110, width: 260, height: 260, borderRadius: 130, backgroundColor: tints.info }} />
        <Appear>
          <View style={{ gap: 12 }}>
            <View style={{ flexDirection: "row" }}>
              <Badge label="headerTransparent" tone="accent" />
            </View>
            <Text style={{ fontSize: 30, fontWeight: "700", letterSpacing: -0.5, color: colors.text }}>Content under the header</Text>
            <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>
              The circles start at the top of the screen; the bar and its back button float over them.
            </Text>
          </View>
        </Appear>
        <Appear index={1}>
          <Inset>
            <Insets />
          </Inset>
        </Appear>
      </View>
    </Screen>
  );
}

function CustomHeader() {
  const likes = signal(0);
  return (
    <Screen
      title="Custom"
      headerTintColor={colors.onAccent}
      headerBackgroundColor={colors.accentFill}
      headerTitleColor={colors.onAccent}
      headerBackVisible={false}
      headerLeft={
        <Text onPress={() => nav.pop()} hitSlop={10} style={{ color: colors.onAccent, fontSize: 17, fontWeight: "500" }}>
          Close
        </Text>
      }
      headerTitle={
        <View style={{ flexDirection: "row", alignItems: "center", gap: 7 }}>
          <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: colors.onAccent }} />
          <Text style={{ color: colors.onAccent, fontSize: 17, fontWeight: "700" }}>Custom title</Text>
        </View>
      }
      headerRight={
        <View onPress={() => likes.set(likes() + 1)} hitSlop={10} style={{ flexDirection: "row", alignItems: "center", gap: 5 }}>
          <Icon name="star" size={18} tint={colors.onAccent} />
          <Text style={{ color: colors.onAccent, fontSize: 17, fontWeight: "600", fontVariant: ["tabular-nums"] }}>{likes()}</Text>
        </View>
      }
    >
      <Page intro="The header's items are views: the star on the right counts presses and re-renders the header.">
        <Section title="Header" note="headerRight">
          <Figure value={String(likes())} unit={likes() === 1 ? "star" : "stars"} />
        </Section>
      </Page>
    </Screen>
  );
}

function Locked() {
  return (
    <Screen title="No swipe back" {...header} gestureEnabled={false}>
      <Page
        intro={hint(
          "The back gesture is off on this screen: no swipe on iOS, no predictive animation on Android. The back buttons still work.",
          "The back gesture is off on this screen (touch screens only). The back button, Escape and Alt+Left still work.",
        )}
      >
        <Section title="Leaving" note="gestureEnabled={false}">
          <View style={{ flexDirection: "row" }}>
            <Stat width="50%" label="Swipe / predictive" value="Off" tone="danger" />
            <Stat width="50%" label="Back buttons" value="On" tone="up" />
          </View>
        </Section>
      </Page>
    </Screen>
  );
}

/** `preventBack`: leaving with unsaved text asks first. */
function Unsaved() {
  const text = signal("");
  const focused = signal(false);
  const confirm = async () => {
    const i = await Alert.alert("Discard your changes?", undefined, [
      { text: "Keep editing", style: "cancel" },
      { text: "Discard", style: "destructive" },
    ]);
    if (i === 1) nav.pop();
  };
  return (
    <Screen title="Unsaved changes" {...header} preventBack={text().length > 0} onBackRequest={confirm}>
      <Page
        intro={hint(
          "Type something, then go back (header button, swipe, Android back): the screen asks first.",
          "Type something, then go back (header button, Escape, Alt+Left, the mouse's back button): the screen asks first.",
        )}
      >
        <Card icon="form" title="Draft" subtitle="preventBack + onBackRequest" right={<Badge label={text().length > 0 ? "Guarded" : "Empty"} tone={text().length > 0 ? "warn" : "neutral"} />}>
          <Input
            value={text()}
            onChangeText={(v) => text.set(v)}
            onFocus={() => focused.set(true)}
            onBlur={() => focused.set(false)}
            placeholder="A draft"
            placeholderTextColor={colors.faint}
            selectionColor={colors.accent}
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
          <Text style={{ fontSize: 13, color: colors.muted }}>
            {text().length > 0 ? `${text().length} characters would be lost: back asks first` : "Nothing to lose: back pops"}
          </Text>
        </Card>
      </Page>
    </Screen>
  );
}

function Faded() {
  return (
    <Screen title="Fade" {...header} animation="fade">
      <Page intro="This screen faded in, and fades out." />
    </Screen>
  );
}

function Instant() {
  return (
    <Screen title="No animation" {...header} animation="none">
      <Page intro="Shown without a transition, and removed without one." />
    </Screen>
  );
}

function Headerless() {
  return (
    <Screen headerShown={false}>
      <SafeAreaView style={{ flex: 1, backgroundColor: colors.background }}>
        <View style={{ flex: 1, padding: 16, gap: 16 }}>
          <Appear>
            <View style={{ gap: 8, paddingHorizontal: 4, paddingTop: 12 }}>
              <View style={{ flexDirection: "row" }}>
                <Badge label="headerShown={false}" tone="accent" />
              </View>
              <Text style={{ fontSize: 30, fontWeight: "700", letterSpacing: -0.5, color: colors.text }}>No header</Text>
              <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>
                The screen owns the whole surface; a SafeAreaView keeps it clear of the status bar and the home indicator.
              </Text>
            </View>
          </Appear>
          <Appear index={1}>
            <Section title="Safe area" note="SafeAreaView">
              <Insets />
            </Section>
          </Appear>
          <View style={{ flex: 1 }} />
          <Button label="Back" onPress={() => nav.pop()} />
        </View>
      </SafeAreaView>
    </Screen>
  );
}

function Deep(props: { depth: number }) {
  return (
    <Screen title={`Level ${props.depth}`} {...header}>
      <Page intro="Every push is a native screen; popToTop() unwinds them all at once.">
        <Card icon="nav" title={`Level ${props.depth}`} subtitle={`${props.depth} deep in the stack`}>
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 6 }}>
            <Each of={Array.from({ length: props.depth }, (_, i) => i + 1)}>
              {(level) => (
                <View
                  style={{
                    width: 32,
                    height: 32,
                    borderRadius: 8,
                    alignItems: "center",
                    justifyContent: "center",
                    borderWidth: level() === props.depth ? 0 : 1,
                    borderColor: colors.border,
                    backgroundColor: level() === props.depth ? colors.accentFill : colors.raised,
                    ...(level() === props.depth ? finish.accent : finish.raised),
                  }}
                >
                  <Text style={{ fontSize: 13, fontWeight: "600", fontVariant: ["tabular-nums"], color: level() === props.depth ? colors.onAccent : colors.muted }}>{level()}</Text>
                </View>
              )}
            </Each>
          </View>
          <View style={{ flexDirection: "row", gap: 8 }}>
            <Button flex={1} label="Deeper" icon="plus" onPress={() => nav.push({ name: "page", page: () => <Deep depth={props.depth + 1} /> })} />
            <Button flex={1} label="To the index" variant="secondary" onPress={() => nav.popToTop()} />
          </View>
        </Card>
      </Page>
    </Screen>
  );
}

type Sheet = "fullScreen" | "pageSheet" | "formSheet" | "fade" | "dialog" | "dismissible" | null;

const modals: { key: Exclude<Sheet, null>; title: string; note: string; icon: IconName }[] = [
  { key: "fullScreen", title: "Full screen", note: "Slides up", icon: "mobile" },
  { key: "pageSheet", title: "Page sheet", note: hint("iOS sheet", "A tall centered card"), icon: "components" },
  { key: "formSheet", title: "Form sheet", note: hint("iOS, centered", "A smaller centered card"), icon: "form" },
  { key: "fade", title: "Fade", note: "Fades in", icon: "spark" },
  { key: "dialog", title: "Dialog", note: "See-through", icon: "bell" },
  { key: "dismissible", title: "Dismissible", note: hint("Swipe to close", "Escape or click outside"), icon: "arrow-down" },
];

/** A modal's content: what it is, its safe area, a close button. */
function ModalBody(props: { title: string; note: string; body: string; onClose: () => void }) {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: colors.background }}>
      <View style={{ flex: 1, padding: 16, gap: 16 }}>
        <View style={{ gap: 8, paddingHorizontal: 4, paddingTop: 8 }}>
          <View style={{ flexDirection: "row" }}>
            <Badge label={props.note} tone="accent" />
          </View>
          <Text style={{ fontSize: 28, fontWeight: "700", letterSpacing: -0.5, color: colors.text }}>{props.title}</Text>
          <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>{props.body}</Text>
        </View>
        <Section title="Safe area" note="the modal's">
          <Insets />
        </Section>
        <View style={{ flex: 1 }} />
        <Button label="Close" onPress={props.onClose} />
      </View>
    </SafeAreaView>
  );
}

export function Navigation() {
  const sheet = signal<Sheet>(null);
  const intercept = signal(false);
  const backs = signal(0);
  const close = () => sheet.set(null);
  // A listener hears the button before the stack pops it; listening only
  // while intercepting keeps Android's back gesture previewing the pop.
  effect(() => {
    if (!intercept()) return;
    const back = BackHandler.addEventListener("hardwareBackPress", () => {
      backs.set(backs() + 1);
      return true;
    });
    cleanup(() => back.remove());
  });
  const push = (page: () => JSX.Element) => () => nav.push({ name: "page", page });
  // The dialog springs up from a little smaller each time it opens.
  const pop = createAnimatedValue(0);
  effect(() => {
    if (sheet() !== "dialog") return;
    pop.set(0);
    pop.spring(1, motion.settle);
  });
  const current = () => modals.find((m) => m.key === sheet());

  return (
    <DemoPage
      cards={[
        () => (
          <Card icon="nav" title="Native stack" subtitle="Headers, transitions, the safe area" right={<Badge label={hint("UIKit / View", Platform.OS === "linux" ? "GTK 4" : "Native views")} />}>
            <Insets />
          </Card>
        ),
        () => (
          <Card title="Screens" subtitle="Screen options, pushed with nav.push" flush>
            <Inset padding={0} gap={0}>
              <LinkRow first icon="text" title="Large title" note="ios.headerLargeTitle" onPress={push(LargeTitle)} />
              <LinkRow icon="styles" title="Transparent header" note="headerTransparent" onPress={push(Transparent)} />
              <LinkRow icon="star" title="Custom header items" note="headerLeft / Title / Right" onPress={push(CustomHeader)} />
              <LinkRow icon="mobile" title="No header" note="headerShown={false}" onPress={push(Headerless)} />
              <LinkRow icon="spark" title="Fade in" note='animation="fade"' onPress={push(Faded)} />
              <LinkRow icon="play" title="No animation" note='animation="none"' onPress={push(Instant)} />
              <LinkRow icon="gestures" title="No swipe back" note="gestureEnabled={false}" onPress={push(Locked)} />
              <LinkRow icon="form" title="Unsaved changes" note="preventBack + onBackRequest" onPress={push(Unsaved)} />
              <LinkRow icon="lists" title="A deep stack" note="push / popToTop" onPress={push(() => <Deep depth={1} />)} />
            </Inset>
          </Card>
        ),
        () => (
          <Card title="Modals" subtitle="Presentation styles and transitions" flush>
            <View style={{ flexDirection: "row", flexWrap: "wrap", justifyContent: "space-between", rowGap: 8 }}>
              <Each of={modals}>
                {(m) => (
                  <Press onPress={() => sheet.set(m().key)} accessibilityLabel={m().title} style={{ width: "48.5%" }}>
                    <View
                      style={{
                        minHeight: 100,
                        padding: 12,
                        gap: 12,
                        borderRadius: radius.inset,
                        borderCurve: "continuous",
                        backgroundColor: colors.inset,
                        borderWidth: 1,
                        borderColor: colors.line,
                        ...finish.inset,
                      }}
                    >
                      <IconTile name={m().icon} size={32} />
                      <View style={{ gap: 2 }}>
                        <Text style={{ fontSize: 15, fontWeight: "500", color: colors.text }}>{m().title}</Text>
                        <Text style={{ fontSize: 13, color: colors.faint }}>{m().note}</Text>
                      </View>
                    </View>
                  </Press>
                )}
              </Each>
            </View>
          </Card>
        ),
        () => (
          <Card
            icon="system"
            title="Back button"
            subtitle={hint("Android: BackHandler", "BackHandler: Escape, Alt+Left")}
            right={<Toggle value={intercept()} onChange={(v) => intercept.set(v)} label="Intercept the back button" />}
          >
            <Figure value={String(backs())} unit={backs() === 1 ? "press" : "presses"} note={intercept() ? "caught" : "popped"} tone={intercept() ? "warn" : "accent"} />
            <Text style={{ fontSize: 13, lineHeight: 18, color: colors.muted }}>
              {intercept()
                ? "The handler returns true: back presses stay on this screen."
                : hint("Off: the back button pops this screen.", "Off: Escape pops this screen, or the app's last screen does nothing.")}
            </Text>
          </Card>
        ),
      ]}
    >
      <Modal
        visible={sheet() === "fullScreen" || sheet() === "pageSheet" || sheet() === "formSheet" || sheet() === "fade"}
        ios={{ presentationStyle: sheet() === "pageSheet" ? "pageSheet" : sheet() === "formSheet" ? "formSheet" : "fullScreen" }}
        desktop={{ presentationStyle: sheet() === "pageSheet" ? "pageSheet" : sheet() === "formSheet" ? "formSheet" : "fullScreen" }}
        web={{ presentationStyle: sheet() === "pageSheet" ? "pageSheet" : sheet() === "formSheet" ? "formSheet" : "fullScreen" }}
        animationType={sheet() === "fade" ? "fade" : "slide"}
        onRequestClose={close}
      >
        <ModalBody
          title={current()?.title ?? ""}
          note={String(sheet())}
          body={hint(
            "Close with the button, the back button (Android) or a swipe down (iOS sheets).",
            sheet() === "pageSheet" || sheet() === "formSheet" ? "Close with the button, Escape or a click outside the card." : "Close with the button or Escape.",
          )}
          onClose={close}
        />
      </Modal>
      <Modal
        visible={sheet() === "dismissible"}
        dismissible
        ios={{ presentationStyle: "pageSheet" }}
        desktop={{ presentationStyle: "pageSheet" }}
        web={{ presentationStyle: "pageSheet" }}
        onRequestClose={close}
      >
        <ModalBody
          title="Dismissible"
          note="dismissible"
          body={hint("Swipe it down (iOS) or press back (Android): it closes natively, then tells the app.", "Press Escape or click outside: it closes natively, then tells the app.")}
          onClose={close}
        />
      </Modal>
      <Modal visible={sheet() === "dialog"} transparent animationType="fade" onRequestClose={close}>
        <View style={{ flex: 1, backgroundColor: "rgba(0,0,0,0.62)", alignItems: "center", justifyContent: "center", padding: 16 }}>
          <View
            style={{
              alignSelf: "stretch",
              opacity: pop,
              transform: [{ scale: pop.interpolate({ inputRange: [0, 1], outputRange: [0.9, 1] }) }],
            }}
          >
            <Card icon="bell" title="A dialog" subtitle="transparent + fade">
              <Text style={{ fontSize: 15, lineHeight: 21, color: colors.muted }}>
                A transparent modal shows what is behind it; the dialog springs in natively.
              </Text>
              <View style={{ flexDirection: "row", gap: 8 }}>
                <Button flex={1} label="Cancel" variant="secondary" onPress={close} />
                <Button flex={1} label="OK" onPress={close} />
              </View>
            </Card>
          </View>
        </View>
      </Modal>
    </DemoPage>
  );
}
