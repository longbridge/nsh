// A case of the API's test suite (docs/dev/compatibility.md): `nsh/android`
// off Android (the headless runtime is iOS): `isAvailable` is false, and its
// functions are callable and do nothing, so an app that calls them runs on.
// Kept with `cargo xtask api --keep api/cases/mod-android.tsx`; its script
// checks each.
import { Text, View, mount, signal } from "nsh";
import { BackHandler, Toast, isAvailable, setStatusBarBackgroundColor } from "nsh/android";

function App() {
  const calls = signal("not called");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Android: ${isAvailable}`}</Text>
      <Text>{`Exit: ${typeof BackHandler.exitApp}`}</Text>
      <Text>{`Calls: ${calls()}`}</Text>
      <View
        testID="call"
        onPress={() => {
          const listener = BackHandler.addEventListener("hardwareBackPress", () => true);
          listener.remove();
          Toast.show("Saved");
          Toast.show("Saved for long", "long");
          setStatusBarBackgroundColor("#336699", true);
          calls.set(`done, remove ${typeof listener.remove}`);
        }}
      >
        <Text>Call them</Text>
      </View>
    </View>
  );
}

mount(App);
