// A case of the API's test suite (docs/dev/compatibility.md): `nsh/ios` on
// the headless runtime, which is iOS: `isAvailable` is true, and an
// `InputAccessoryView` takes no room where it is placed. Its script checks each.
import { Input, Text, View } from "nsh";
import { InputAccessoryView, isAvailable } from "nsh/ios";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`iOS: ${isAvailable}`}</Text>
      <View testID="column">
        <View testID="above" style={{ height: 30 }} />
        <InputAccessoryView nativeID="bar">
          <View style={{ height: 44 }}>
            <Text>Accessory bar</Text>
          </View>
        </InputAccessoryView>
        <View testID="below" style={{ height: 30 }} />
      </View>
      <Input ios={{ inputAccessoryViewID: "bar" }} style={{ height: 40 }} />
      <Text>Rendered</Text>
    </View>
  );
}

export default App;
