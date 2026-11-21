import { keymap, mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";
import { Editor, type EditorRef } from "nsh/base";

// An editor that hands the app the keys it runs a query, formats and
// explains with, for `editor-key-bindings.test.tsx`; the app also binds
// mod+enter itself, for while the editor does not have the focus.

function App() {
  const keys = signal<string[]>(["mod+enter", "mod+shift+f", "escape", "not a key", "mod+e"]);
  const taken = signal<string[]>([]);
  const app = signal(0);
  const text = signal("select 1");
  let editor!: EditorRef;
  keymap.bind("mod+enter", () => app.set(app() + 1));
  const button = (id: string, onPress: () => void) => (
    <View testID={id} style={{ width: 8, height: 8 }} onPress={onPress} />
  );
  return (
    <View style={{ flex: 1, paddingTop: 20 }}>
      <View style={{ flexDirection: "row" }}>
        {button("release", () => keys.set(["mod+shift+f"]))}
        {button("blur", () => editor.blur())}
      </View>
      <Text>{() => `taken ${taken().join(",") || "-"} app ${app()} lines ${text().split("\n").length}`}</Text>
      <View style={{ width: 400, height: 200 }}>
        <Editor
          ref={editor}
          testID="sql"
          defaultValue="select 1"
          keyBindings={keys()}
          onKeyBinding={({ keystroke }) => taken.set([...taken(), keystroke])}
          onChange={({ value }) => text.set(value)}
          style={{ flex: 1 }}
        />
      </View>
    </View>
  );
}

mount(() => <Window id="main">{App}</Window>);
