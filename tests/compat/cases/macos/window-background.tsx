// A case of the API's test suite (docs/dev/compatibility.md): the window's
// background appearance (`Window.setBackgroundAppearance`), which macOS
// shows and every other desktop keeps opaque. This case checks that every
// value is callable. The macOS backend's main-thread integration tests
// check the native backdrop.
import { Text, View, signal } from "nsh";
import { Window } from "nsh/desktop";

function WindowBackground() {
  const appearance = signal<"opaque" | "blurred">("opaque");
  const set = (value: "opaque" | "blurred") => {
    Window.setBackgroundAppearance(value);
    appearance.set(value);
  };
  return (
    <View style={{ flex: 1 }}>
      <Text>{`Background: ${appearance()}`}</Text>
      <View testID="blurred" style={{ width: 20, height: 20 }} onPress={() => set("blurred")} />
      <View testID="opaque" style={{ width: 20, height: 20 }} onPress={() => set("opaque")} />
    </View>
  );
}

export default WindowBackground;
