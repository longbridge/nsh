// A case of the API's test suite (docs/dev/compatibility.md): what
// `nsh/ui/mobile` shows over the app: a `Modal` shown and hidden, a `Sheet`
// opened and closed by the app (its own close animation ends on a
// platform's display frames, which the headless runtime has none of), and
// the platform's dialogs (`Alert`, `ActionSheet`), whose promises the
// headless runtime leaves pending (no user answers them). Kept with
// `cargo xtask api --keep api/cases/mod-ui-overlays.tsx`; its script checks
// each.
import { Text, View, mount, signal } from "nsh";
import { ActionSheet, Alert, Button, Modal, Sheet } from "nsh/ui/mobile";

function App() {
  const modal = signal(false);
  const sheet = signal(false);
  const closed = signal(0);
  const alert = signal("not shown");
  const actions = signal("not shown");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Button title="Show the modal" onPress={() => modal.set(true)} />
      <Button title="Open the sheet" onPress={() => sheet.set(true)} />
      <Text>{`Sheet closed: ${closed()}`}</Text>
      <Button
        title="Alert"
        onPress={() => {
          const shown = Alert.alert("Saved", "Your changes were saved.", [{ text: "Later", style: "cancel" }, { text: "Delete", style: "destructive" }, { text: "OK" }], {
            cancelable: true,
          });
          alert.set(typeof shown.then === "function" ? "pending" : "no promise");
          shown.then((i) => alert.set(`pressed ${i}`), (e) => alert.set(`failed ${e}`));
          Alert.alert("Plain").catch((e) => alert.set(`failed ${e}`));
        }}
      />
      <Text>{`Alert: ${alert()}`}</Text>
      <Button
        title="Actions"
        onPress={() => {
          const shown = ActionSheet.show({ title: "Share", message: "Pick one", options: ["Cancel", "Copy", "Delete"], cancelButtonIndex: 0, destructiveButtonIndexes: [2] });
          actions.set(typeof shown.then === "function" ? "pending" : "no promise");
          shown.then((i) => actions.set(`chose ${i}`), (e) => actions.set(`failed ${e}`));
        }}
      />
      <Text>{`Actions: ${actions()}`}</Text>
      <Modal visible={modal()} animationType="none" onRequestClose={() => modal.set(false)}>
        <View style={{ flex: 1, paddingTop: 80 }}>
          <Text>In the modal</Text>
          <Button title="Hide the modal" onPress={() => modal.set(false)} />
        </View>
      </Modal>
      <Sheet
        open={sheet()}
        onClose={() => {
          sheet.set(false);
          closed.set(closed() + 1);
        }}
      >
        <Text>In the sheet</Text>
        <Button title="Done" onPress={() => sheet.set(false)} />
      </Sheet>
    </View>
  );
}

mount(App);
