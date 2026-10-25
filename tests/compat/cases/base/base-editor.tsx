// A case of the API's test suite (docs/dev/compatibility.md): `Editor`
// of `nsh/base`, a code editor whose text is its own (`defaultValue`):
// the ref moves the selections (`setSelections`), types (`insert`), runs
// commands (`run("undo")`, `run("redo")`), replaces the search's matches
// (`replaceAll`) and reads the text back (`value`, `selections`,
// `canUndo`, `canRedo`); each edit is reported with the whole new text
// (`onChange`), each move of the carets with every selection
// (`onSelectionChange`), and the matches of `search` with their count
// (`onSearchChange`). The mouse resting over a word reports a hover
// (`onHover`, `onHoverEnd`); the ref tells where the caret and the text
// in view are (`caretBounds`, `visibleRange`, `scrollOffset`), converts
// offsets (`positionToOffset`, `offsetToPosition`), reads the selected
// text (`selectedText`) and goes to a line (`goToLine`).
import { Text, View, signal } from "nsh";
import { Editor, type EditorRef } from "nsh/base";

export default function BaseEditor() {
  const value = signal("none");
  const caret = signal("none");
  const found = signal("none");
  const history = signal("none");
  const read = signal("none");
  const query = signal("");
  const hovered = signal("none");
  let editor!: EditorRef;
  const track = async () => history.set(`Undo ${await editor.canUndo()} Redo ${await editor.canRedo()}`);
  return (
    <View style={{ paddingTop: 20 }}>
      <View style={{ width: 390, height: 160 }}>
        <Editor
          ref={r => (editor = r)}
          defaultValue="let x = 1;"
          language="rust"
          search={{ query: query(), caseSensitive: true }}
          accessibilityLabel="Code"
          onChange={v => value.set(v)}
          onSelectionChange={selections => {
            const head = selections[selections.length - 1].head;
            caret.set(`${head.line}:${head.column}`);
          }}
          onSearchChange={result => found.set(`${result.count}`)}
          onHover={e =>
            hovered.set(
              `${e.position.line}:${e.position.column} ${e.range.start.column}-${e.range.end.column} ${e.diagnostics.length}`,
            )
          }
          onHoverEnd={() => hovered.set("ended")}
          style={{ flex: 1 }}
        />
      </View>
      <Text>{() => `Value: ${value()}`}</Text>
      <Text>{() => `Caret: ${caret()}`}</Text>
      <Text>{() => `Found: ${found()}`}</Text>
      <Text>{() => `History: ${history()}`}</Text>
      <Text>{() => `Read: ${read()}`}</Text>
      <Text>{() => `Hover: ${hovered()}`}</Text>
      <View
        testID="to-end"
        style={{ height: 20 }}
        onPress={() => editor.setSelections([{ anchor: { line: 0, column: 10 }, head: { line: 0, column: 10 } }])}
      />
      <View
        testID="insert"
        style={{ height: 20 }}
        onPress={() => {
          editor.insert(" // ok");
          track();
        }}
      />
      <View
        testID="undo"
        style={{ height: 20 }}
        onPress={() => {
          editor.run("undo");
          track();
        }}
      />
      <View
        testID="redo"
        style={{ height: 20 }}
        onPress={() => {
          editor.run("redo");
          track();
        }}
      />
      <View testID="find" style={{ height: 20 }} onPress={() => query.set("x")} />
      <View testID="replace-all" style={{ height: 20 }} onPress={async () => read.set(`replaced ${await editor.replaceAll("y")}`)} />
      <View
        testID="read"
        style={{ height: 20 }}
        onPress={async () => read.set(`${await editor.value()} (${(await editor.selections()).length} selection)`)}
      />
      <View
        testID="geometry"
        style={{ height: 20 }}
        onPress={async () => {
          const caret = await editor.caretBounds();
          const visible = await editor.visibleRange();
          const scroll = await editor.scrollOffset();
          const offset = await editor.positionToOffset({ line: 0, column: 4 });
          const back = await editor.offsetToPosition(offset);
          read.set(
            `caret ${caret ? caret.bounds.size.height : "none"} lines ${visible.start.line}-${visible.end.line} scroll ${scroll.y} offset ${offset} ${back.line}:${back.column}`,
          );
        }}
      />
      <View
        testID="select-word"
        style={{ height: 20 }}
        onPress={async () => {
          editor.setSelections([{ anchor: { line: 0, column: 4 }, head: { line: 0, column: 5 } }]);
          read.set(`selected ${await editor.selectedText()}`);
        }}
      />
      <View testID="go-to-line" style={{ height: 20 }} onPress={() => editor.goToLine(0)} />
    </View>
  );
}
