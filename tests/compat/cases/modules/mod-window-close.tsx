// The headless surface has no native close button. Listener registration,
// removal and argument checks remain available; native tests drive close actions.
import { Text, View, signal } from "nsh";
import { Window } from "nsh/desktop";

export default function WindowClose() {
  const result = signal("not called");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Window close calls: ${result()}`}</Text>
      <View testID="window-close-policy" onPress={() => {
        Window.setCloseOnPress(false);
        const stop = Window.on("closePress", () => {});
        stop();
        stop();
        Window.setCloseOnPress(true);
        let refused = 0;
        try {
          // @ts-expect-error The event name is checked at runtime too.
          Window.on("closed", () => {});
        } catch (error) {
          if (error instanceof TypeError) refused++;
        }
        try {
          // @ts-expect-error Only callable listeners are accepted.
          Window.on("closePress", 1);
        } catch (error) {
          if (error instanceof TypeError) refused++;
        }
        result.set(`done, stop=${typeof stop}, refused=${refused}`);
      }}>
        <Text>Configure window closing</Text>
      </View>
    </View>
  );
}
