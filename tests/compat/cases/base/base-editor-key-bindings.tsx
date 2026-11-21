// A case of the API's test suite (docs/dev/compatibility.md): an `Editor`
// that hands the app keys it binds itself while it has the focus
// (`keyBindings`), reported by `onKeyBinding`. Its own mod+enter would
// insert a line below; the app's takes it instead, so the text keeps one
// line.
import { Text, View, signal } from "nsh";
import { Editor } from "nsh/base";

export default function BaseEditorKeyBindings() {
  const taken = signal("none");
  const lines = signal(1);
  return (
    <View style={{ paddingTop: 20 }}>
      <View style={{ width: 300, height: 120 }}>
        <Editor
          defaultValue="select 1"
          keyBindings={["mod+enter"]}
          onKeyBinding={({ keystroke }) => taken.set(keystroke)}
          onChange={({ value }) => lines.set(value.split("\n").length)}
          style={{ flex: 1 }}
        />
      </View>
      <Text>{() => `Taken: ${taken()}`}</Text>
      <Text>{() => `Lines: ${lines()}`}</Text>
    </View>
  );
}
