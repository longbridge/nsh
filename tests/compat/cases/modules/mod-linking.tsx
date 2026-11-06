// A case of the API's test suite (docs/dev/compatibility.md):
// `Linking` on the headless runtime, where the system opens the app with no
// URL: `on("url", …)` returns a function that stops listening and hears
// nothing. Its script checks it.
import { Linking, Text, View, signal } from "nsh";

function App() {
  const heard = signal("none");
  const stop = Linking.on("url", ({ url }) => heard.set(url));
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Link: ${heard()}`}</Text>
      <View testID="stop" onPress={() => (stop(), heard.set(`stopped ${typeof stop}`))}>
        <Text>Stop</Text>
      </View>
    </View>
  );
}

export default App;
