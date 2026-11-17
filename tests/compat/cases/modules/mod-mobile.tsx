// A case of the API's test suite (docs/dev/compatibility.md): `nsh/mobile`
// on the headless runtime, which is iOS: `isAvailable` is true, and
// `TabBar`, the system's tab bar on iOS and Android: its tabs (`items`, each `{ title, icon, badge }`), the selected one
// (`selectedIndex`), which the app sets, and `onChange`, which the
// platform reports with the index of the tab the user chose. The bar takes
// the frame the app's style gives it, as a native view does; its script
// checks the frame and the app's side of the selection. `SegmentedControl`,
// the system's segmented control on iOS: its segments (`items`, each
// `{ title, icon, disabled }`), the selected one (`selectedIndex`), which
// the app sets, `disabled`, and `onChange` with the index the user chose.
// `DateTimePicker`, the system's date and time picker on iOS and Android:
// what it picks (`mode`), its value and bounds (`value`, `min`, `max`: ISO
// strings in the mode's form), `minuteInterval`, `disabled`, and
// `onChange({ value })` with the user's pick.
import { Text, View, signal } from "nsh";
import { DateTimePicker, SegmentedControl, TabBar, isAvailable } from "nsh/mobile";

function App() {
  const selected = signal(0);
  const when = signal("2026-11-17T09:30");
  const items = signal([
    { title: "Records", icon: "list.bullet" },
    { title: "Stats", icon: "chart.bar", badge: "3" },
  ]);
  return (
    <View style={{ flex: 1, paddingTop: 20 }}>
      <Text>{`Mobile: ${isAvailable}`}</Text>
      <Text>{`Tab: ${selected()}`}</Text>
      <Text testID="tab-next" onPress={() => selected.set(1)}>Next tab</Text>
      <Text testID="tab-rename" onPress={() => items.set([{ title: "Records", icon: "list.bullet" }, { title: "Statistics", icon: "chart.bar", badge: "" }])}>
        Rename
      </Text>
      <SegmentedControl
        testID="segments"
        items={[{ title: "Light", icon: "sun.max" }, { title: "Dark" }, { title: "Auto", disabled: true }]}
        selectedIndex={selected()}
        onChange={({ index }) => selected.set(index)}
        style={{ marginHorizontal: 16, height: 32 }}
      />
      <Text>{`When: ${when()}`}</Text>
      <DateTimePicker
        testID="when"
        mode="dateTime"
        value={when()}
        min="2026-11-01T00:00"
        max="2027-11-01T00:00"
        minuteInterval={5}
        disabled={false}
        onChange={({ value }) => when.set(value)}
        style={{ marginHorizontal: 16 }}
      />
      <TabBar
        testID="tab-bar"
        items={items()}
        selectedIndex={selected()}
        onChange={({ index }) => selected.set(index)}
        style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 60 }}
      />
    </View>
  );
}

export default App;
