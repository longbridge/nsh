// Focus rings drawn with `focus-visible:` (focus-visible.test.tsx): a view
// that shows a marker only while its focus shows, a button that moves the
// focus to it (`focus()`), and a date picker whose today shows the same
// marker while the keyboard is on it. A view in `display: none` is not on
// screen, so the markers tell the test what a ring would show.
import { mount, Text, View, type ViewRef } from "nsh";
import { DatePicker } from "nsh/base";

let target: ViewRef | undefined;

function App() {
  return (
    <View style={{ flex: 1, padding: 24, gap: 12, alignItems: "flex-start" }}>
      <View role="button" onPress={() => target?.focus()} style={{ padding: 8 }}>
        <Text>Focus the target</Text>
      </View>
      <View
        ref={(r) => (target = r)}
        testID="target"
        role="button"
        class="group"
        onPress={() => {}}
        style={{ padding: 8 }}
      >
        <Text>Target</Text>
        <Text class="hidden group-focus-visible:flex">target ring</Text>
      </View>
      <DatePicker
        accessibilityLabel="Due date"
        itemClass="group"
        renderItem={(item) =>
          item.kind === "day" && item.today ? (
            <View>
              <Text>{String(item.value)}</Text>
              <Text class="hidden group-focus-visible:flex">today ring</Text>
            </View>
          ) : (
            <Text>{String(item.value)}</Text>
          )
        }
        trigger={
          <View style={{ padding: 8 }}>
            <Text>Pick a date</Text>
          </View>
        }
      />
    </View>
  );
}

mount(App);
