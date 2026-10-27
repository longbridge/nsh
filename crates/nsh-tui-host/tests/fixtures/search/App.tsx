import { mount, Scroll, signal, Text, View } from "nsh";

function App() {
  const focused = signal("none"), pressed = signal("none");
  const labels = Array.from({ length: 20 }, (_, index) => `row${index}`);
  return <View style={{ flex: 1 }}>
    <Text style={{ height: 16 }}>focus:{focused()} pressed:{pressed()}</Text>
    <Scroll style={{ width: 160, height: 64 }}>
      {labels.map((label) => <View style={{ height: 16, flexShrink: 0 }}
        onFocus={() => focused.set(label)} onPress={() => pressed.set(label)}><Text>{label}</Text></View>)}
    </Scroll>
    <Text style={{ position: "absolute", top: 144 }}>footer</Text>
  </View>;
}
mount(App);
