// A case of the API's test suite (docs/dev/compatibility.md): a read-only
// `RichEditor` reports text typed into it with `onReadonlyInput` (the text
// and whether an input method committed it) and keeps it out of the
// document. Its script types through the editor's native input.
import { Text, View, signal } from "nsh";
import { RichEditor, StarterKit, createRichEditor } from "nsh/base";

function App() {
  const editor = createRichEditor({ extensions: [StarterKit], content: "Notes" });
  const heard = signal<string[]>([]);
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <RichEditor
        editor={editor}
        editable={false}
        accessibilityLabel="ReadonlyNotes"
        style={{ height: 120 }}
        onReadonlyInput={({ text, composed }) => heard.set([...heard(), `${text}:${composed}`])}
      />
      <Text>{`Heard: ${heard().join(" ")}`}</Text>
      <Text>{`Doc: ${editor.getText()}`}</Text>
    </View>
  );
}

export default App;
