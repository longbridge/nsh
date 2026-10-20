// A case of the API's test suite (docs/dev/compatibility.md): a script run
// as on macOS (`platform macos`), where `desktop={{…}}` props apply and
// `ios={{…}}` ones do not. Kept with
// `cargo xtask api --keep api/cases/desktop-props.tsx`.
import { Text, View, mount } from "nsh";

function App() {
  return (
    <View style={{ flex: 1 }}>
      <View testID="bar" desktop={{ windowDrag: true }} style={{ height: 44 }}>
        <Text>Title bar</Text>
      </View>
      <View testID="body" desktop={{ windowDrag: false }} style={{ flex: 1 }} />
    </View>
  );
}

mount(App);
