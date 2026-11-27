import { Text, View, signal } from "nsh";
import type { ViewRef } from "nsh";

function App() {
  let article: ViewRef | undefined;
  const selected = signal("");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View ref={(node) => (article = node)} selectable onSelectionChange={({ text }) => selected.set(text)}>
        <Text>Selectable article</Text>
      </View>
      <Text>{`Selected: ${selected() || "none"}`}</Text>
      <View testID="select-all" onPress={() => article?.selectAll()}>
        <Text>Select all</Text>
      </View>
      <View testID="clear" onPress={() => article?.clearSelection()}>
        <Text>Clear</Text>
      </View>
    </View>
  );
}

export default App;
