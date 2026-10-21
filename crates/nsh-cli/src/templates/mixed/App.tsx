import { mount, signal, Text } from "nsh";
import { SafeAreaView } from "nsh/base";
import { Badge, Counter, Format } from "__SPECIFIER__";

// Its count lives in Rust (`src/lib.rs`).
const counter = new Counter();

function App() {
  const count = signal(counter.count);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <Text style={{ fontSize: 22 }}>{count() ? `Pressed ${Format.times(count())}` : "Not pressed yet"}</Text>
      <Badge label="Press me" onPress={() => count.set(counter.increment())} />
    </SafeAreaView>
  );
}

mount(App);
