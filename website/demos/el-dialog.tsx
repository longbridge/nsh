import { mount, signal, Text, View } from "nsh";
import { Button, Dialog, SafeAreaView, Switch } from "nsh/base";
import { desktop, Inspector, Panel, size, Stage, tint, ui, wide } from "./palette";

const track = `${desktop ? "w-9" : "w-12"} rounded-full p-0.5 transition-colors`;
const thumb = desktop ? "size-4 rounded-full transition-transform checked:translate-x-4" : "size-5.5 rounded-full transition-transform checked:translate-x-5";

function App() {
  const card = signal(false);
  const sheet = signal(false);
  const backdropCloses = signal(true);
  const cancels = signal(0);
  const log = signal("–");
  // A pointer gets compact buttons as wide as their labels; a phone, 44-point ones.
  const compact = { borderRadius: 8, height: size.control, paddingHorizontal: 14, alignItems: "center" as const, justifyContent: "center" as const };
  const primary = () => (desktop ? { ...ui().primary, ...compact } : { ...ui().primary, borderRadius: 10, padding: 12, alignItems: "center" as const });
  const secondary = () => (desktop ? { ...ui().raised, ...compact } : { ...ui().raised, borderRadius: 10, padding: 12, alignItems: "center" as const });
  const buttonText = desktop ? 14 : 16;
  const openCard = () => (
    <Button onPress={() => card.set(true)} style={primary()} activeStyle={{ opacity: 0.7 }}>
      <Text style={{ color: ui().onPrimary, fontSize: buttonText, fontWeight: "600" }}>Open a card</Text>
    </Button>
  );
  const openSheet = () => (
    <Button onPress={() => sheet.set(true)} style={secondary()} activeStyle={{ opacity: 0.7 }}>
      <Text style={{ color: ui().text, fontSize: buttonText, fontWeight: "600" }}>Open a sheet</Text>
    </Button>
  );
  const backdrop = () => (
    <Switch
      checked={backdropCloses()}
      onChange={(next) => backdropCloses.set(next)}
      class={wide() ? "flex-row items-center gap-2 self-center active:opacity-70" : "flex-row items-center gap-3 rounded-xl p-3 active:opacity-70"}
      style={wide() ? undefined : ui().row}
      trackClass={track}
      trackStyle={backdropCloses() ? ui().accentFill : ui().inset}
      thumbClass={thumb}
      thumbStyle={backdropCloses() ? { backgroundColor: ui().onAccent } : ui().raised}
    >
      <Text style={{ color: ui().text, flex: wide() ? undefined : 1, fontSize: desktop ? 13 : undefined }}>closeOnBackdropPress {String(backdropCloses())}</Text>
    </Switch>
  );
  const hint = desktop
    ? "Close with Escape or a click on the backdrop."
    : "Close with a tap on the backdrop, a swipe down or the back button. On iOS the sheet is a system pageSheet.";
  const state = () => `open: card ${String(card())} · sheet ${String(sheet())} · onCancel ${cancels()}×`;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>dialog</Text>

      {() =>
        wide() ? (
          <>
            <Text style={{ color: ui().muted }}>{hint}</Text>
            <View style={{ flex: 1, flexDirection: "row", gap: 16 }}>
              <Stage gap={16}>
                <View style={{ flexDirection: "row", justifyContent: "center", gap: 8 }}>
                  {openCard()}
                  {openSheet()}
                </View>
                {backdrop()}
              </Stage>
              <Inspector>
                <Panel title="open · onCancel">
                  <Text style={{ fontSize: 13, color: ui().text }}>{state()}</Text>
                </Panel>
                <Panel title="onOpenChange(open, reason) · onOpen" grow>
                  <Text style={{ fontSize: 13, fontWeight: "600", color: ui().accent }}>{log()}</Text>
                </Panel>
              </Inspector>
            </View>
          </>
        ) : (
          <View style={{ gap: 10 }}>
            {openCard()}
            {openSheet()}

            {backdrop()}

            <View style={{ ...ui().inset, borderRadius: 12, padding: 14, gap: 4 }}>
              <Text style={{ color: ui().muted, fontSize: 13 }}>{state()}</Text>
              <Text style={{ color: ui().accent, fontSize: 15, fontWeight: "600" }}>{log()}</Text>
            </View>
            <Text style={{ color: ui().faint, fontSize: 13 }}>{hint}</Text>
          </View>
        )
      }

      <Dialog
        open={card()}
        animation="fade"
        closeOnBackdropPress={backdropCloses()}
        onOpenChange={(open, reason) => {
          card.set(open);
          log.set(`onOpenChange(${open}, "${reason}")`);
        }}
        onCancel={() => cancels.update((n) => n + 1)}
        onOpen={() => log.set("onOpen: card")}
        backdropTestID="card-backdrop"
        backdropStyle={{ backgroundColor: tint("#000000", 0.45) }}
        style={{ ...ui().card, margin: "auto", width: 320, maxWidth: "90%", borderRadius: 18, padding: 20, gap: 12 }}
      >
        <Text style={{ color: ui().text, fontSize: 20, fontWeight: "700" }}>Delete this item?</Text>
        <Text style={{ color: ui().muted }}>
          {backdropCloses()
            ? "A press on the dimmed backdrop closes it."
            : "The backdrop only calls onCancel: close with a button."}
        </Text>
        <View style={{ flexDirection: "row", gap: 8, justifyContent: desktop ? "flex-end" : undefined }}>
          <Button
            onPress={() => {
              card.set(false);
              log.set("closed by the app: Keep");
            }}
            style={desktop ? secondary() : { ...secondary(), flex: 1 }}
            activeStyle={{ opacity: 0.7 }}
          >
            <Text style={{ color: ui().text, fontWeight: "600" }}>Keep</Text>
          </Button>
          <Button
            onPress={() => {
              card.set(false);
              log.set("closed by the app: Delete");
            }}
            style={desktop ? { ...compact, backgroundColor: ui().down } : { flex: 1, borderRadius: 10, padding: 12, alignItems: "center", backgroundColor: ui().down }}
            activeStyle={{ opacity: 0.7 }}
          >
            <Text style={{ color: "#ffffff", fontWeight: "600" }}>Delete</Text>
          </Button>
        </View>
      </Dialog>

      <Dialog
        open={sheet()}
        presentation="pageSheet"
        onOpenChange={(open, reason) => {
          sheet.set(open);
          log.set(`onOpenChange(${open}, "${reason}")`);
        }}
        onCancel={() => cancels.update((n) => n + 1)}
        onOpen={() => log.set("onOpen: sheet")}
        backdropStyle={{ backgroundColor: tint("#000000", 0.3) }}
        style={{
          backgroundColor: ui().page,
          flex: 1,
          marginTop: 40,
          width: "100%",
          maxWidth: 560,
          alignSelf: "center",
          borderTopLeftRadius: 16,
          borderTopRightRadius: 16,
        }}
      >
        <SafeAreaView style={{ flex: 1, padding: 20, gap: 12 }}>
          <Text style={{ color: ui().text, fontSize: 22, fontWeight: "700" }}>A sheet</Text>
          <Text style={{ color: ui().muted }}>
            presentation="pageSheet": a system sheet on iOS that follows the finger when swiped down. Elsewhere the
            dialog is a layer over the window, and this panel is the app's own style.
          </Text>
          <Button onPress={() => sheet.set(false)} style={desktop ? { ...primary(), alignSelf: "flex-end" } : primary()} activeStyle={{ opacity: 0.7 }}>
            <Text style={{ color: ui().onPrimary, fontSize: buttonText, fontWeight: "600" }}>Close</Text>
          </Button>
        </SafeAreaView>
      </Dialog>
    </View>
  );
}

mount(App);
