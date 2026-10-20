// A case of the API's test suite (docs/dev/compatibility.md): what an input
// reports as the user edits it: focus and blur, each key, the text after
// each edit and the selection; the size of a multiline input's text as it
// wraps onto more lines; a controlled input whose app changes what was
// typed, which typing then continues from. Each handler shows what it gets
// as text. Kept with `cargo xtask api --keep api/cases/events-input.tsx`;
// its script checks each.
import { Input, Text, View, mount, signal } from "nsh";

function App() {
  const focus = signal("none");
  const keys = signal<string[]>([]);
  const text = signal("");
  const selection = signal("none");
  const size = signal("none");
  let first = -1;
  const upper = signal("");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Input
        testID="field"
        style={{ width: 300, height: 40 }}
        onFocus={() => focus.set("focused")}
        onBlur={() => focus.set(focus() + " blurred")}
        onKeyPress={(e) => keys.update((k) => [...k, e.key])}
        onChangeText={(t) => text.set(t)}
        onSelectionChange={(e) => selection.set(`${e.start}-${e.end}`)}
      />
      <Text>Focus: {focus()}</Text>
      <Text>Keys: {keys().join(" ")}</Text>
      <Text>Text: {text()}</Text>
      <Text>Selection: {selection()}</Text>
      <Input
        testID="notes"
        multiline
        style={{ width: 100 }}
        onContentSizeChange={(e) => {
          if (first < 0) first = e.height;
          const typed = typeof e.width === "number" && typeof e.height === "number";
          size.set(
            `${Object.keys(e).sort().join(",")} ${typed ? "numbers" : "not numbers"} ` +
              `${e.height > first ? "taller" : "as tall"}`,
          );
        }}
      />
      <Text>Size: {size()}</Text>
      <Input
        testID="upper"
        value={upper()}
        style={{ width: 300, height: 40 }}
        onChangeText={(t) => upper.set(t.toUpperCase())}
      />
      <Text>Upper: {upper()}</Text>
    </View>
  );
}

mount(App);
