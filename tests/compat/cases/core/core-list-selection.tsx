import { List, Text, View, signal } from "nsh";

function App() {
  const selected = signal<number | null>(1);
  const button = (testID: string, label: string, index: number | null) => (
    <View testID={testID} onPress={() => selected.set(index)} style={{ height: 20 }}>
      <Text>{label}</Text>
    </View>
  );
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      {button("select-3", "Select 3", 3)}
      {button("select-far", "Select 40", 40)}
      <List
        testID="picks"
        each={Array.from({ length: 50 }, (_, i) => i)}
        estimatedItemSize={40}
        style={{ width: 300, height: 200 }}
        selectedIndex={selected()}
        selectionIndicatorClass="mx-[8px] my-[2px]"
        selectionIndicatorStyle={{ backgroundColor: "#ff0000" }}
      >
        {(row) => (
          <View testID={`pick-${row()}`} style={{ height: 40 }}>
            <Text>{`Pick ${row()}`}</Text>
          </View>
        )}
      </List>
    </View>
  );
}

export default App;
