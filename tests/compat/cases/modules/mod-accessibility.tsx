// A case of the API's test suite (docs/dev/compatibility.md):
// `nsh/accessibility` as the headless runtime reports it (no screen reader,
// full motion), its promises and listeners, and the calls that speak or move
// a screen reader's focus going through. Its script
// checks each.
import { Text, View, signal } from "nsh";
import { AccessibilityInfo, reduceMotion, screenReaderEnabled } from "nsh/accessibility";

function App() {
  const reader = signal("waiting");
  const motion = signal("waiting");
  const calls = signal("not called");
  let note: any;
  AccessibilityInfo.isScreenReaderEnabled().then((on) => reader.set(String(on)));
  AccessibilityInfo.isReduceMotionEnabled().then((on) => motion.set(String(on)));
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Screen reader: ${screenReaderEnabled()}`}</Text>
      <Text>{`Reduce motion: ${reduceMotion()}`}</Text>
      <Text>{`Reader promise: ${reader()}`}</Text>
      <Text>{`Motion promise: ${motion()}`}</Text>
      <Text ref={note}>{`Calls: ${calls()}`}</Text>
      <View
        testID="call"
        onPress={() => {
          const a = AccessibilityInfo.addEventListener("screenReaderChanged", () => {});
          const b = AccessibilityInfo.addEventListener("reduceMotionChanged", () => {});
          a.remove();
          b.remove();
          AccessibilityInfo.announceForAccessibility("Hello");
          AccessibilityInfo.setAccessibilityFocus(note);
          calls.set("done");
        }}
      >
        <Text>Call them</Text>
      </View>
    </View>
  );
}

export default App;
