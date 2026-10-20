// A case of the API's test suite (docs/dev/compatibility.md): the props of
// every element inside `android={{…}}`, on Android (the script's
// platform): `ripple`, `elevation` and `accessibilityLiveRegion`. Kept with
// `cargo xtask api --keep api/cases/props-view-android.tsx`; its script
// checks each.
import { View, mount } from "nsh";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View
        testID="ripple"
        onPress={() => {}}
        android={{ ripple: { color: "#ff0000", borderless: true, radius: 20 } }}
        style={{ height: 40 }}
      />
      <View
        testID="card"
        android={{ elevation: 4 }}
        style={{ height: 40, backgroundColor: "#ffffff" }}
      />
      <View testID="live" android={{ accessibilityLiveRegion: "polite" }} style={{ height: 40 }} />
      <View testID="plain" style={{ height: 40 }} />
    </View>
  );
}

mount(App);
