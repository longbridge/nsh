// A case of the API's test suite (docs/dev/compatibility.md): what the
// platform reports when a context menu is asked for (the secondary
// button, a long press): `onContextMenu`'s positions and modifier keys,
// heard by the nearest listener around the view under the pointer. The
// handler shows what it gets as text. Kept with
// `cargo xtask api --keep api/cases/events-context-menu.tsx`; its script
// checks each.
import { Text, View, mount, signal } from "nsh";

function App() {
  const menu = signal("none");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View style={{ paddingLeft: 20 }}>
        <View
          testID="menu"
          style={{ height: 60, paddingTop: 10 }}
          onContextMenu={(e) =>
            menu.set(
              `at ${e.locationX},${e.locationY} page ${e.pageX},${e.pageY}` +
                `${e.altKey ? " alt" : ""}${e.ctrlKey ? " ctrl" : ""}` +
                `${e.metaKey ? " meta" : ""}${e.shiftKey ? " shift" : ""}`,
            )
          }
        >
          <View testID="inner" style={{ height: 20 }} />
        </View>
      </View>
      <Text>Menu: {menu()}</Text>
    </View>
  );
}

mount(App);
