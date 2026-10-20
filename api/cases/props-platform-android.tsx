// A case of the API's test suite (docs/dev/compatibility.md): a script run
// as on Android (`platform android`), where `android={{…}}` props apply:
// one view's material shadow, beside one without. Kept with
// `cargo xtask api --keep api/cases/props-platform-android.tsx`; its script
// checks it.
import { Text, View, mount } from "nsh";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View testID="card" android={{ elevation: 6 }} style={{ height: 60 }}>
        <Text>Card</Text>
      </View>
      <View testID="flat" style={{ height: 60 }}>
        <Text>Flat</Text>
      </View>
    </View>
  );
}

mount(App);
