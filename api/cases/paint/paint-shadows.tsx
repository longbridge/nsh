// A case of the API's test suite (docs/dev/compatibility.md): shadows as
// the platform receives them: React Native's shadow keys, CSS box shadows
// (offsets, blur, spread, inset, colors in several notations, several at
// once, `none`) and the text shadow keys of a text.
import { Text, View } from "nsh";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>Shadows</Text>
      <View
        testID="shadow"
        style={{
          height: 8,
          shadowColor: "#000",
          shadowOffset: { width: 2, height: 4 },
          shadowOpacity: 0.3,
          shadowRadius: 6,
        }}
      />
      <View
        testID="shadow-red"
        style={{
          height: 8,
          shadowColor: "rgba(255, 0, 0, 0.5)",
          shadowOffset: { width: -1, height: 0 },
          shadowOpacity: 1,
          shadowRadius: 0,
        }}
      />
      <View testID="box" style={{ height: 8, boxShadow: "2px 4px 6px 1px #00000080" }} />
      <View testID="box-two" style={{ height: 8, boxShadow: "0 1px 2px rgba(0, 0, 0, 0.2), inset 0 0 0 1px red" }} />
      <View testID="box-offsets" style={{ height: 8, boxShadow: "3px 3px blue" }} />
      <View testID="box-first" style={{ height: 8, boxShadow: "hsl(0, 100%, 50%) -2px -3px 5px" }} />
      <View testID="box-inset" style={{ height: 8, boxShadow: "inset 1px 2px 3px 4px #0f08" }} />
      <View testID="box-none" style={{ height: 8, boxShadow: "none" }} />
      <Text
        testID="text-shadow"
        style={{ textShadowColor: "#336699", textShadowOffset: { width: 1, height: 2 }, textShadowRadius: 3 }}
      >
        Shadowed
      </Text>
      <Text
        testID="text-shadow-alpha"
        style={{ textShadowColor: "rgba(0, 0, 0, 0.5)", textShadowOffset: { width: -2, height: 0 }, textShadowRadius: 0 }}
      >
        Faint
      </Text>
    </View>
  );
}

export default App;
