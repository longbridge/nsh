import { List, mount, signal, Text, View, type TextSelectionEndpoint } from "nsh";
import { TextView } from "nsh/base";

type Row = {
  id?: string;
  key?: string;
  text?: string;
  markdown?: string;
  inner?: { id: string; text: string }[];
};

// Rows a list without a keyExtractor names by their key, id or index: a
// plain text, a markdown list and a list of its own. A title outside the
// list is selectable with them.
const rows: Row[] = [
  { id: "x", text: "Row with an id." },
  { key: "k", text: "Row with a key." },
  { text: "Row by its index." },
  { id: "m", markdown: "- First item\n- Second item" },
  {
    id: "n",
    inner: [
      { id: "i1", text: "Inner one." },
      { id: "i2", text: "Inner two." },
    ],
  },
];

function App() {
  const ends = signal("none");
  const end = (e?: TextSelectionEndpoint) => (e ? `${e.key}@${e.offset}` : "-");
  return (
    <View style={{ flex: 1, padding: 16, gap: 8 }}>
      <View
        selectable
        style={{ height: 400 }}
        onSelectionChange={(e) => ends.set(e.text ? `${end(e.anchor)} ${end(e.focus)} ${JSON.stringify(e.text)}` : "none")}
      >
        <Text style={{ width: 154 }}>Transcript title</Text>
        <List each={rows} style={{ flex: 1 }}>
          {(row) => (
            <View style={{ paddingVertical: 8, alignItems: "flex-start" }}>
              {row().inner ? (
                <List each={row().inner!} keyExtractor={(inner) => inner.id} style={{ height: 70, width: 300 }}>
                  {(inner) => (
                    <View style={{ paddingVertical: 4 }}>
                      <Text style={{ width: 154 }}>{inner().text}</Text>
                    </View>
                  )}
                </List>
              ) : row().markdown ? (
                <TextView markdown={row().markdown!} style={{ width: 300 }} />
              ) : (
                <Text style={{ width: 154 }}>{row().text}</Text>
              )}
            </View>
          )}
        </List>
      </View>
      <Text>{`Ends: ${ends()}`}</Text>
    </View>
  );
}

mount(App);
