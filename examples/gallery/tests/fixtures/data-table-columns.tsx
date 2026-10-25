import { mount, signal, Text, View } from "nsh";
import { DataTable } from "nsh/base";

const columns = [
  { key: "name", title: "Name", width: 200, cellClass: "justify-end", headerClass: "justify-end" },
  { key: "note", title: "Note", width: 80 },
];
const rows = [
  { name: "Ada", note: "a much longer text than the width" },
  { name: "Alan", note: "short" },
];

function App() {
  const widths = signal("");
  return (
    <View style={{ flex: 1, paddingTop: 20 }}>
      <Text>{() => `widths ${widths()}`}</Text>
      <View style={{ width: 400, height: 200 }}>
        <DataTable
          testID="table"
          columns={columns}
          data={rows}
          rowCount={rows.length}
          onColumnResize={(key, width) => widths.set(`${key}=${Math.round(width)}`)}
          style={{ flex: 1 }}
        />
      </View>
    </View>
  );
}

mount(App);
