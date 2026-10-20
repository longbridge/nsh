// A case of the API's test suite (docs/dev/compatibility.md): what the
// platform reports of a finger, the mouse, dragged files and assistive
// technologies: press in and out, hover in, moves over and out, file drags
// entering, leaving and dropping, and accessibility actions. Each handler
// shows what it gets as text. Kept with
// `cargo xtask api --keep api/cases/events-pointer.tsx`; its script checks
// each.
import { Text, View, mount, signal } from "nsh";

function App() {
  const press = signal("none");
  const hover = signal("none");
  const drop = signal("none");
  const action = signal("none");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View
        testID="press"
        style={{ height: 40 }}
        onPressIn={() => press.set("in")}
        onPressOut={() => press.set(press() + " out")}
      />
      <Text>Press: {press()}</Text>
      <View style={{ paddingLeft: 20 }}>
        <View
          testID="hover"
          style={{ height: 40 }}
          onHoverIn={() => hover.set("in")}
          onHoverMove={(e) =>
            hover.set(
              `move ${e.locationX},${e.locationY} page ${e.pageX},${e.pageY}` +
                `${e.altKey ? " alt" : ""}${e.ctrlKey ? " ctrl" : ""}` +
                `${e.metaKey ? " meta" : ""}${e.shiftKey ? " shift" : ""}`,
            )
          }
          onHoverOut={() => hover.set(hover() + " out")}
        />
      </View>
      <Text>Hover: {hover()}</Text>
      <View
        testID="drop"
        style={{ height: 40 }}
        onFileDragEnter={() => drop.set("enter")}
        onFileDragLeave={() => drop.set(drop() + " leave")}
        onFileDrop={(e) => drop.set(`dropped ${e.uris.length}: ${e.uris.join(" ")}`)}
      />
      <Text>Drop: {drop()}</Text>
      <View
        testID="action"
        accessible
        accessibilityLabel="Counter"
        accessibilityActions={[{ name: "increment", label: "Increment" }, { name: "activate" }]}
        onAccessibilityAction={(e) => action.set(e.actionName)}
        style={{ height: 40 }}
      />
      <Text>Action: {action()}</Text>
    </View>
  );
}

mount(App);
