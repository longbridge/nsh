// A case of the API's test suite (docs/dev/compatibility.md):
// `Location` on the headless runtime, where no platform reports a position
// and this app declares no capability: `available()` resolves false,
// `authorization()` resolves "notDetermined", and both calls that would read
// a position refuse with `ERR_ACCESS_DENIED` — location is never granted
// without a declaration. Its script checks each.
import { Text, View, signal } from "nsh";
import { Location } from "nsh";

function App() {
  const available = signal("unknown");
  const authorization = signal("unknown");
  const current = signal("not read");
  const watch = signal("not started");
  Location.available().then((yes: boolean) => available.set(String(yes)));
  Location.authorization().then((value: string) => authorization.set(value));
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Available: ${available()}`}</Text>
      <Text>{`Authorization: ${authorization()}`}</Text>
      <Text>{`Current: ${current()}`}</Text>
      <Text>{`Watch: ${watch()}`}</Text>
      <View
        testID="read"
        onPress={() => {
          Location.current({ accuracy: "high", timeout: 100 }).then(
            (position) => current.set(`${position.coords.latitude},${position.coords.longitude}`),
            (error) => current.set(String(error.code ?? error.message)),
          );
          try {
            Location.watch({ distanceFilter: 1 }, () => {});
            watch.set("watching");
          } catch (error) {
            watch.set(String((error as { code?: string }).code ?? error));
          }
        }}
      >
        <Text>Read</Text>
      </View>
    </View>
  );
}

export default App;
