// A case of the API's test suite (docs/dev/compatibility.md): a script run
// as in a phone's browser (`platform web`), where `web={{…}}` props apply
// (any prop of the element) and `ios={{…}}` / `android={{…}}` ones do not.
// Kept with `cargo xtask api --keep api/cases/props-platform-web.tsx`; its
// script checks it.
import { Text, View, mount } from "nsh";
import type { WebProps } from "nsh";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View
        testID="link"
        accessibilityLabel="Everywhere"
        web={{ accessibilityLabel: "In a browser" } as WebProps}
        android={{ elevation: 6 }}
        style={{ height: 60 }}
      >
        <Text>Link</Text>
      </View>
    </View>
  );
}

mount(App);
