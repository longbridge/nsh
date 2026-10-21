import { mount, signal, Text, View } from "nsh";
import { AlertDialog, Button } from "nsh/base";
import { platform } from "nsh/device";
import { ui } from "./palette";

function App() {
  const answer = signal("–");
  const shown = signal(0);
  const show = async (options: Parameters<typeof AlertDialog.show>[0]) => {
    shown.update((n) => n + 1);
    const choice = await AlertDialog.show(options);
    answer.set(choice === null ? "null (closed without an action)" : JSON.stringify(choice));
  };
  const row = () => ({ ...ui().row, borderRadius: 12, padding: 14, gap: 2 });
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>alert-dialog</Text>
      <Text style={{ color: ui().muted }}>The platform's own alert; every label comes from the app.</Text>

      <View style={{ maxWidth: 390, gap: 10 }}>
        <Button
          onPress={() =>
            show({
              title: "Delete the photo?",
              description: "This cannot be undone.",
              actions: [
                { label: "Cancel", role: "cancel" },
                { label: "Delete", role: "destructive", id: "delete" },
              ],
            })
          }
          style={row()}
          activeStyle={{ opacity: 0.7 }}
        >
          <Text style={{ color: ui().down, fontSize: 16, fontWeight: "600" }}>Delete…</Text>
          <Text style={{ color: ui().muted, fontSize: 13 }}>cancel + destructive roles</Text>
        </Button>

        <Button
          onPress={() =>
            show({
              title: "Save changes?",
              description: "Your edits to “Trip notes” are not saved yet.",
              actions: [
                { label: "Don't save", role: "destructive", id: "discard" },
                { label: "Cancel", role: "cancel" },
                { label: "Save", id: "save" },
              ],
              closeOnEscape: true,
            })
          }
          style={row()}
          activeStyle={{ opacity: 0.7 }}
        >
          <Text style={{ color: ui().text, fontSize: 16, fontWeight: "600" }}>Save changes…</Text>
          <Text style={{ color: ui().muted, fontSize: 13 }}>three actions, closeOnEscape</Text>
        </Button>

        <Button
          onPress={() =>
            show({
              title: "Share the photo",
              presentation: "actionSheet",
              actions: [
                { label: "Copy link", id: "copy" },
                { label: "Save", id: "save" },
                { label: "Delete", role: "destructive", id: "delete" },
                { label: "Cancel", role: "cancel" },
              ],
            })
          }
          style={row()}
          activeStyle={{ opacity: 0.7 }}
        >
          <Text style={{ color: ui().text, fontSize: 16, fontWeight: "600" }}>Share…</Text>
          <Text style={{ color: ui().muted, fontSize: 13 }}>presentation "actionSheet"</Text>
        </Button>

        <View style={{ ...ui().inset, borderRadius: 12, padding: 14, gap: 4, marginTop: 4 }}>
          <Text style={{ color: ui().muted, fontSize: 13 }}>AlertDialog.show resolved with ({shown()} shown)</Text>
          <Text style={{ color: ui().accent, fontSize: 16, fontWeight: "600" }}>{answer()}</Text>
        </View>
        <Text style={{ color: ui().faint, fontSize: 13 }}>
          {platform.idiom === "desktop"
            ? "Escape or a click outside the action sheet closes it with null."
            : "A tap outside the action sheet closes it with null."}
        </Text>
      </View>
    </View>
  );
}

mount(App);
