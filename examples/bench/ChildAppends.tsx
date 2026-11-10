// Child insertion benchmark. Set COUNT to compare sibling counts, and
// MOUNTED to append after the empty container's first commit.
import { mount, signal, Text, View } from "nsh";
import { Button } from "nsh/base";

const COUNT = 1000;
const MOUNTED = false;

function App() {
  const ready = signal(!MOUNTED);
  return (
    <View style={{ flex: 1 }}>
      <Button testID="append" onPress={() => ready.set(true)}>
        <Text>Append children</Text>
      </Button>
      <View testID="children">
        {() => ready() ? Array.from({ length: COUNT }, () =>
          <View style={{ width: 40, height: 16 }} />
        ) : null}
      </View>
    </View>
  );
}

mount(App);
