import { mount, signal, Text, View } from "nsh";
import { Button, Dialog, SafeAreaView } from "nsh/base";
import { platform } from "nsh/device";
import { tint, ui } from "./palette";

function App() {
  const card = signal(false);
  const sheet = signal(false);
  const backdropCloses = signal(true);
  const cancels = signal(0);
  const log = signal("–");
  const primary = () => ({ ...ui().primary, borderRadius: 10, padding: 12, alignItems: "center" as const });
  const secondary = () => ({ ...ui().raised, borderRadius: 10, padding: 12, alignItems: "center" as const });
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>dialog</Text>

      <View style={{ maxWidth: 390, gap: 10 }}>
        <Button onPress={() => card.set(true)} style={primary()} activeStyle={{ opacity: 0.7 }}>
          <Text style={{ color: ui().onPrimary, fontSize: 16, fontWeight: "600" }}>Open a card</Text>
        </Button>
        <Button onPress={() => sheet.set(true)} style={secondary()} activeStyle={{ opacity: 0.7 }}>
          <Text style={{ color: ui().text, fontSize: 16, fontWeight: "600" }}>Open a sheet</Text>
        </Button>

        <Button
          role="switch"
          selected={backdropCloses()}
          onPress={() => backdropCloses.set(!backdropCloses())}
          style={{ ...ui().row, borderRadius: 12, padding: 12, flexDirection: "row", alignItems: "center", gap: 10 }}
          activeStyle={{ opacity: 0.7 }}
        >
          <View
            style={{
              width: 20,
              height: 20,
              borderRadius: 6,
              borderWidth: 1,
              borderColor: ui().border,
              backgroundColor: backdropCloses() ? ui().accent : "transparent",
            }}
          />
          <Text style={{ color: ui().text, flex: 1 }}>closeOnBackdropPress {String(backdropCloses())}</Text>
        </Button>

        <View style={{ ...ui().inset, borderRadius: 12, padding: 14, gap: 4 }}>
          <Text style={{ color: ui().muted, fontSize: 13 }}>
            open: card {String(card())} · sheet {String(sheet())} · onCancel {cancels()}×
          </Text>
          <Text style={{ color: ui().accent, fontSize: 15, fontWeight: "600" }}>{log()}</Text>
        </View>
        <Text style={{ color: ui().faint, fontSize: 13 }}>
          {platform.idiom === "desktop"
            ? "Close with Escape or a click on the backdrop."
            : "Close with a tap on the backdrop, a swipe down or the back button. On iOS the sheet is a system pageSheet."}
        </Text>
      </View>

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
        <View style={{ flexDirection: "row", gap: 8 }}>
          <Button
            onPress={() => {
              card.set(false);
              log.set("closed by the app: Keep");
            }}
            style={{ ...secondary(), flex: 1 }}
            activeStyle={{ opacity: 0.7 }}
          >
            <Text style={{ color: ui().text, fontWeight: "600" }}>Keep</Text>
          </Button>
          <Button
            onPress={() => {
              card.set(false);
              log.set("closed by the app: Delete");
            }}
            style={{ flex: 1, borderRadius: 10, padding: 12, alignItems: "center", backgroundColor: ui().down }}
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
          <Button onPress={() => sheet.set(false)} style={primary()} activeStyle={{ opacity: 0.7 }}>
            <Text style={{ color: ui().onPrimary, fontSize: 16, fontWeight: "600" }}>Close</Text>
          </Button>
        </SafeAreaView>
      </Dialog>
    </View>
  );
}

mount(App);
