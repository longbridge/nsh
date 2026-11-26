// A case of the API's test suite (docs/dev/compatibility.md): a tap on a
// RichEditor table's column grip reports its table and column
// (`onTableGripPress`), where an app opens its own menu for the column.
import { Text, View, signal } from "nsh";
import { RichEditor, StarterKit, TableKit, createRichEditor } from "nsh/base";

function App() {
  const editor = createRichEditor({
    extensions: [StarterKit, TableKit],
    format: "markdown",
    content: "Above\n\n| a1 | a2 |\n| --- | --- |\n| b1 | b2 |",
  });
  const pressed = signal("GripPress none");
  return (
    <View style={{ flex: 1, paddingTop: 60, paddingLeft: 20 }}>
      <RichEditor
        editor={editor}
        style={{ width: 300, height: 200, flexShrink: 0 }}
        onTableGripPress={(e) =>
          pressed.set(
            `GripPress ${e.table} ${e.row ?? "-"} ${e.column ?? "-"} ${e.pageBounds.size.width > 0} ${editor.selection.type}`,
          )
        }
      />
      <Text>{pressed()}</Text>
    </View>
  );
}

export default App;
