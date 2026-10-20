// What a platform does besides drawing, as headless runs play it: going
// back, dialogs, the clipboard, native animations, a modal; and a request
// only a real platform answers.
import { mount, signal, Text, View } from "nsh";
import { Animated, createAnimatedValue } from "nsh/animated";
import { Clipboard } from "nsh/clipboard";
import { Image } from "nsh/image";
import { Alert, createNavigator, Modal, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "home" } | { name: "detail" } | { name: "draft" };
const nav = createNavigator<Route>({ name: "home" });

function Press(props: { label: string; onPress: () => void }) {
  return (
    <View onPress={props.onPress}>
      <Text>{props.label}</Text>
    </View>
  );
}

function Home() {
  const answer = signal("none");
  const pasted = signal("");
  const moved = signal("still");
  const sheet = signal(false);
  const shown = signal(0);
  const x = createAnimatedValue(0);
  return (
    <Screen title="Home">
      <View style={{ flex: 1 }}>
        <Text>{`Answer: ${answer()}`}</Text>
        <Text>{`Pasted: ${pasted()}`}</Text>
        <Text>{`Moved: ${moved()}`}</Text>
        <Text>{`Shown: ${shown()}`}</Text>
        <Press label="Open" onPress={() => nav.push({ name: "detail" })} />
        <Press label="Draft" onPress={() => nav.push({ name: "draft" })} />
        <Press
          label="Delete"
          onPress={async () => {
            const i = await Alert.alert("Delete?", "It is gone after.", [
              { text: "Cancel", style: "cancel" },
              { text: "Delete", style: "destructive" },
            ]);
            answer.set(["Cancel", "Delete"][i] ?? "dismissed");
          }}
        />
        <Press
          label="Update"
          onPress={async () => {
            const i = await Alert.alert("Update", "Install it now?", [{ text: "Later" }, { text: "Install" }]);
            answer.set(["Later", "Install"][i] ?? "dismissed");
          }}
        />
        <Press label="Copy" onPress={() => Clipboard.setString("copied text")} />
        <Press label="Paste" onPress={async () => pasted.set(await Clipboard.getString())} />
        <Press label="Move" onPress={() => Animated.timing(x, { toValue: 100, duration: 300 }).start(() => moved.set("done"))} />
        <Press label="Measure" onPress={() => Image.getSize("https://example.com/a.png")} />
        <Press label="Sheet" onPress={() => sheet.set(true)} />
        <Modal visible={sheet()} onRequestClose={() => sheet.set(false)} onShow={() => shown.set(shown() + 1)}>
          <Text>In the sheet</Text>
        </Modal>
      </View>
    </Screen>
  );
}

function Draft() {
  const asked = signal(false);
  return (
    <Screen title="Draft" preventBack onBackRequest={() => asked.set(true)}>
      <Text>{`Asked: ${asked() ? "yes" : "no"}`}</Text>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) =>
        route.name === "home" ? (
          <Home />
        ) : route.name === "draft" ? (
          <Draft />
        ) : (
          <Screen title="Detail">
            <Text>Detail body</Text>
          </Screen>
        )
      }
    </Stack>
  );
}

mount(App);
