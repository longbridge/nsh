// A case of the API's test suite (docs/dev/compatibility.md):
// `nsh/clipboard` on the headless runtime, where no system clipboard
// answers: writing goes through, and every read is a promise (pending
// there). Its script checks each.
import { Text, View, signal } from "nsh";
import { Clipboard } from "nsh/clipboard";

function App() {
  const calls = signal("not called");
  const failed = signal("none");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Calls: ${calls()}`}</Text>
      <Text>{`Failed: ${failed()}`}</Text>
      <View
        testID="copy"
        onPress={() => {
          Clipboard.setString("Copied text");
          Clipboard.setImage(
            "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
          );
          const reads = [Clipboard.getString(), Clipboard.hasString(), Clipboard.getImage(), Clipboard.hasImage(), Clipboard.getFiles()];
          for (const read of reads) read.catch((e) => failed.set(String(e)));
          calls.set(`${reads.filter((r) => typeof r.then === "function").length} promises`);
        }}
      >
        <Text>Copy</Text>
      </View>
    </View>
  );
}

export default App;
