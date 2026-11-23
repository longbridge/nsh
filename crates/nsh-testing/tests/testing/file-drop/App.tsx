// The reproduction of longbridge/nsh#2478: a file the user drops on the app
// is theirs to give it, as a picked one is, so `readFile` of it needs no
// `capabilities.fs.read` root for its folder. This app declares read (and
// write) of `${dataDir}` only; the test drops a file from the OS temp dir,
// outside it, holding `# Hallo`.
import { mount, signal, Text, View } from "nsh";
import { readFile } from "fs/promises";

function App() {
  const out = signal("");
  return (
    <View
      testID="area"
      class="flex-1"
      onFileDrop={(e) => {
        const path = decodeURIComponent(new URL(e.uris[0]).pathname);
        readFile(path, "utf8").then(
          (t) => out.set("ok:" + t),
          (err) => out.set("err:" + String(err?.code ?? err)),
        );
      }}
    >
      <Text testID="out">{out()}</Text>
    </View>
  );
}

mount(App);
