import { Scroll, Text, View, signal } from "nsh";
import type { ViewRef } from "nsh";

// Each form of the options: the call is there and takes them.
function App() {
  const rows: ViewRef[] = [];
  const called = signal("none");
  const button = (id: string, press: () => void) => (
    <View
      testID={id}
      onPress={() => {
        press();
        called.set(id);
      }}
    >
      <Text>{id}</Text>
    </View>
  );
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>Called: {called()}</Text>
      {button("default", () => rows[7].scrollIntoView())}
      {button("center", () => rows[3].scrollIntoView({ block: "center", animated: false }))}
      {button("nearest", () => rows[9].scrollIntoView({ block: "nearest", inline: "nearest" }))}
      {button("end", () => rows[0].scrollIntoView({ block: "end", inline: "start", animated: true }))}
      <Scroll style={{ height: 200 }}>
        {Array.from({ length: 10 }, (_, i) => (
          <View ref={(node) => (rows[i] = node)} style={{ height: 100 }} />
        ))}
      </Scroll>
    </View>
  );
}

export default App;
