// Checks context-menu payloads and the ref helper emitted by the JSX compiler.
// Keep its build with `cargo xtask api --keep api/cases/context-menu-refs.tsx`.
import { Text, View, mount, signal } from "nsh";

function App() {
  const menu = signal("none");
  const result = signal("unchecked");
  const signalRef = signal<unknown>();
  let variableRef: unknown;
  const fields: { view?: unknown } = {};
  let callbackRef: unknown;
  const receiveRef = (node: unknown) => { callbackRef = node; };

  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View
        testID="menu"
        ref={variableRef}
        style={{ height: 40 }}
        onContextMenu={(e) => menu.set(
          `${e.locationX},${e.locationY} page ${e.pageX},${e.pageY} modifiers ${e.altKey},${e.ctrlKey},${e.metaKey},${e.shiftKey}`,
        )}
      />
      <View ref={signalRef} />
      <View ref={fields.view} />
      <View ref={receiveRef} />
      <Text>Menu: {menu()}</Text>
      <View testID="check-refs" onPress={() => {
        const refs = [variableRef, signalRef(), fields.view, callbackRef];
        result.set(refs.every((node) => node != null) && new Set(refs).size === 4 ? "assigned" : "missing");
      }}>
        <Text>Check refs</Text>
      </View>
      <Text>Refs: {result()}</Text>
    </View>
  );
}

mount(App);
