// A case of the API's test suite (docs/dev/compatibility.md): `Tree` of
// `nsh/base`, items that hold items with no look of their own: the roles,
// names and states screen readers get (a tree of tree items, expanded,
// selected, disabled), a press selecting a row and expanding or collapsing
// a folder, the events (`onExpandedChange`, `onSelectedChange`, `onExpand`,
// `onCollapse`), the app's state (`expanded`, `selected`) or the tree's own
// (`defaultExpanded`, `defaultSelected`), `itemClass` with `selected:`, and
// `renderItem` given each item's depth.
import { Text, View, signal } from "nsh";
import { Tree, type TreeItem } from "nsh/base";

const files: TreeItem[] = [
  {
    id: "src",
    label: "src",
    children: [
      { id: "src/lib.rs", label: "lib.rs" },
      { id: "src/ui", label: "ui", children: [{ id: "src/ui/tree.rs", label: "tree.rs" }] },
    ],
  },
  { id: "docs", label: "docs", disabled: true, children: [{ id: "docs/guide.md", label: "guide.md" }] },
  { id: "README.md", label: "README.md" },
];

const notes: TreeItem[] = [
  { id: "notes", label: "notes", children: [{ id: "notes/a", label: "a.md" }] },
  { id: "todo", label: "todo.md" },
];

function App() {
  const expanded = signal<string[]>(["src"]);
  const selected = signal<string | null>(null);
  const heard = signal("none");
  const changes = signal(0);
  return (
    <View style={{ flex: 1, paddingTop: 20, gap: 4 }}>
      <Tree
        testID="files"
        accessibilityLabel="Files"
        items={files}
        expanded={expanded()}
        selected={selected()}
        style={{ height: 160 }}
        itemClass="h-[20px] selected:bg-[#00ff00]"
        onExpandedChange={({ expanded: next }) => {
          changes.set(changes() + 1);
          expanded.set(next);
        }}
        onSelectedChange={({ selected: next }) => selected.set(next)}
        onExpand={({ id }) => heard.set(`expand ${id}`)}
        onCollapse={({ id }) => heard.set(`collapse ${id}`)}
      />
      <Tree
        testID="notes"
        accessibilityLabel="Notes"
        items={notes}
        defaultExpanded={["notes"]}
        defaultSelected="notes/a"
        style={{ height: 80 }}
        renderItem={(entry) => <Text>{`${entry.folder ? "+" : ""}${entry.label}@${entry.depth}`}</Text>}
      />
      <Text>{`Selected: ${selected() ?? "none"}`}</Text>
      <Text>{`Expanded: ${expanded().join(",")}`}</Text>
      <Text>{`Heard: ${heard()}`}</Text>
      <Text>{`Changes: ${changes()}`}</Text>
    </View>
  );
}

export default App;
