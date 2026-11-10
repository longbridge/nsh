// A case of the API's test suite (docs/dev/compatibility.md): the app's
// login item (`Application.loginItem`, `Application.setLoginItem`), with the
// `loginItem` capability the domain declares, in a run that has none
// (headless, as in a view embedded in another app and under `nsh run`):
// reading it resolves `{ status: "unavailable" }`, changing it rejects with
// `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` and a wrong use with a `TypeError`.
// Its script checks each.
import { Text, View, signal } from "nsh";
import { Application } from "nsh/desktop";

function App() {
  const read = signal("not read");
  const set = signal("not set");
  const misuse = signal("not misused");
  Application.loginItem().then(
    (item) => read.set(`${item.status} ${Object.keys(item).join(",")}`),
    (error) => read.set(`rejected ${error?.code}`),
  );
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Login item: ${read()}`}</Text>
      <Text>{`Set: ${set()}`}</Text>
      <Text>{`Misuse: ${misuse()}`}</Text>
      <View
        testID="enable"
        onPress={() =>
          Application.setLoginItem({ enabled: true }).then(
            (item) => set.set(`resolved ${item.status}`),
            (error) => set.set(`${error instanceof Error} ${error?.code}`),
          )
        }
      >
        <Text>Enable</Text>
      </View>
      <View
        testID="misuse"
        onPress={() =>
          Application.setLoginItem({} as { enabled: boolean }).then(
            () => misuse.set("resolved"),
            (error) => misuse.set(error instanceof TypeError ? "TypeError" : `${error}`),
          )
        }
      >
        <Text>Misuse</Text>
      </View>
    </View>
  );
}

export default App;
