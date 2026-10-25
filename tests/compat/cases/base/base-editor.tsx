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
// text (`selectedText`) and goes to a line (`goToLine`). A command
// changes the selection's case (`run("transformToUppercase")`) and another
// reports the selection for a find bar (`run("findWithSelection")`,
// `onFindWithSelection`). A second editor grows with its text between
// `minRows` and `maxRows` and shows its whitespace (`showWhitespaces`). A
// third folds where the app's `foldRanges` say, by level, recursively and
// by hand, reporting the folded ranges (`onFoldChange`, `folds`), its text
// colored by the app's tokens alone (`decorations` with a `token`,
// `highlighting={false}`). A context menu asked for over the text reports
// where in it, and whether a touch selection asked for it (`selection`,
// its box in the root view) or a pointer (`onContextMenu`).
import { Text, View, signal } from "nsh";
import { Editor, type EditorFoldRange, type EditorRef } from "nsh/base";

export default function BaseEditor() {
  const value = signal("none");
  const caret = signal("none");
  const found = signal("none");
  const history = signal("none");
  const read = signal("none");
  const query = signal("");
  const hovered = signal("none");
  const asked = signal("none");
  const folded = signal("none");
  const menu = signal("none");
  let editor!: EditorRef;
  let grown!: EditorRef;
  let outline!: EditorRef;
  const show = (folds: EditorFoldRange[]) => folds.map(f => `${f.startLine}-${f.endLine}`).join(" ") || "-";
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
          onFindWithSelection={q => asked.set(q)}
          onContextMenu={e =>
            menu.set(`${e.position.line}:${e.position.column} ${e.selection ? `touch ${e.selection.width}` : "pointer"}`)
          }
          style={{ flex: 1 }}
        />
      </View>
      <View style={{ width: 390, height: 100 }}>
        <Editor
          ref={r => (outline = r)}
          defaultValue={"mod a {\n  fn b() {\n    c();\n  }\n}\n"}
          language="rust"
          highlighting={false}
          foldRanges={[
            { startLine: 0, endLine: 3 },
            { startLine: 1, endLine: 2 },
          ]}
          decorations={[{ range: { start: { line: 0, column: 0 }, end: { line: 0, column: 3 } }, token: "keyword" }]}
          onFoldChange={folds => folded.set(show(folds))}
          style={{ flex: 1 }}
        />
      </View>
      <Text>{() => `Folded: ${folded()}`}</Text>
      <View testID="fold-level" style={{ height: 20 }} onPress={() => outline.foldLevel(2)} />
      <View testID="fold-recursive" style={{ height: 20 }} onPress={() => outline.foldRecursive(0)} />
      <View testID="unfold" style={{ height: 20 }} onPress={() => outline.unfold(0)} />
      <View testID="unfold-recursive" style={{ height: 20 }} onPress={() => outline.unfoldRecursive(0)} />
      <View testID="fold-range" style={{ height: 20 }} onPress={() => outline.foldRange({ startLine: 2, endLine: 4 })} />
      <View
        testID="set-folds"
        style={{ height: 20 }}
        onPress={async () => {
          outline.setFolds([{ startLine: 0, endLine: 4 }]);
          read.set(`folds ${show(await outline.folds())}`);
        }}
      />
      <Text>{() => `Value: ${value()}`}</Text>
      <Text>{() => `Caret: ${caret()}`}</Text>
      <Text>{() => `Found: ${found()}`}</Text>
      <Text>{() => `History: ${history()}`}</Text>
      <Text>{() => `Read: ${read()}`}</Text>
      <Text>{() => `Hover: ${hovered()}`}</Text>
      <Text>{() => `Asked: ${asked()}`}</Text>
      <Text>{() => `Menu: ${menu()}`}</Text>
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
      <View
        testID="upper"
        style={{ height: 20 }}
        onPress={() => {
          editor.setSelections([{ anchor: { line: 0, column: 0 }, head: { line: 0, column: 3 } }]);
          editor.run("transformToUppercase");
        }}
      />
      <View testID="ask" style={{ height: 20 }} onPress={() => editor.run("findWithSelection")} />
      <View style={{ width: 300 }}>
        <View testID="grow">
          <Editor ref={r => (grown = r)} defaultValue={"a\tb"} minRows={2} maxRows={3} showWhitespaces />
        </View>
      </View>
      <View testID="grow-more" style={{ height: 20 }} onPress={() => grown.setValue("1\n2\n3\n4\n5")} />
    </View>
  );
}
