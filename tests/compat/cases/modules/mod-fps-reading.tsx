// A case of the API's test suite (docs/dev/compatibility.md): `nsh`'s
// `fpsReading()`, the performance monitor's frame rate: `null` while it
// does not measure, as at launch. Its script checks it.
import { fpsReading, Text, View } from "nsh";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Reading: ${fpsReading() ?? "none"}`}</Text>
    </View>
  );
}

export default App;
