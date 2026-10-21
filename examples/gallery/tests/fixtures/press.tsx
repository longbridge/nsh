import { mount, signal, Text, View } from "nsh";

function App() {
  const outer = signal(0);
  const inner = signal(0);
  return (
    <View style={{ flex: 1, paddingTop: 100 }}>
      <View onPress={() => outer.set(outer() + 1)} style={{ padding: 20 }}>
        <View activeStyle={{ opacity: 0.5 }} style={{ padding: 20 }}>
          <Text>Styled child</Text>
        </View>
      </View>
      <View onPress={() => outer.set(outer() + 1)} style={{ padding: 20 }}>
        <View onPress={() => inner.set(inner() + 1)} activeStyle={{ opacity: 0.5 }} style={{ padding: 20 }}>
          <Text>Clickable child</Text>
        </View>
      </View>
      <Text>Outer {outer()}</Text>
      <Text>Inner {inner()}</Text>
    </View>
  );
}

mount(App);
