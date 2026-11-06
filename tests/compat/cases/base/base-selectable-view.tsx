// A case of the API's test suite (docs/dev/compatibility.md):
// `selectable` views of `nsh`, texts that select together: a plain
// `Text`, a `TextView` and a nested selectable `View` inside one area, a
// `userSelect: "none"` label that stays out, and a text outside every
// area. Its script checks which views are areas and what the texts carry,
// and what the areas report of their selection (`onSelectionChange`).
import { signal, Text, View } from "nsh";
import { TextView } from "nsh/base";

function App() {
  const area = signal("none");
  const own = signal("none");
  return (
    <View style={{ flex: 1, paddingTop: 20, gap: 4 }}>
      <Text>{`Area selected: ${area()}`}</Text>
      <Text>{`Own selected: ${own()}`}</Text>
      <View selectable testID="area" onSelectionChange={(e) => area.set(e.text || "nothing")} style={{ gap: 4 }}>
        <Text>Area first text</Text>
        <TextView testID="area-doc" markdown={"Area **document**\n\n- Area item"} />
        <Text style={{ userSelect: "none" }}>Area label</Text>
        <View selectable testID="inner">
          <Text>Inner text</Text>
        </View>
      </View>
      <TextView testID="own" selectable markdown="Own selection" onSelectionChange={(e) => own.set(e.text || "nothing")} />
      <Text>Outside text</Text>
    </View>
  );
}

export default App;
