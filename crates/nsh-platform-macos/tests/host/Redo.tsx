// A rich editor under the Edit menu's Undo and Redo: its keys (Cmd+Z,
// Cmd+Shift+Z) must undo and redo once.
import { mount, View } from "nsh";
import { createRichEditor, RichEditor, StarterKit } from "nsh/base";
import { Menu, Window } from "nsh/desktop";

function App() {
  Menu.setApplicationMenu([
    { label: "Fixture", submenu: [{ role: "quit" }] },
    { label: "Edit", submenu: [{ role: "undo" }, { role: "redo" }] },
  ]);
  const editor = createRichEditor({ extensions: [StarterKit], content: "" });
  return (
    <View style={{ flex: 1, padding: 12, gap: 6 }}>
      <RichEditor testID="editor" editor={editor} style={{ height: 60, borderWidth: 1 }} />
    </View>
  );
}

mount(() => <Window id="main">{App}</Window>);
