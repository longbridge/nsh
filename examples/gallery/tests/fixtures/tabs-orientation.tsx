// A row of tabs with no `orientation` (the default, horizontal) and a
// vertical one, each with the text of the selected tab. `tabs-orientation.test.tsx`
// runs it: the row moves with Right and leaves Down to the page, the column
// with Down and leaves Right to the page.
import { mount, signal, Text, View } from "nsh";
import { Tab, Tabs } from "nsh/base";

function List(props: { name: string; orientation?: "horizontal" | "vertical" }) {
  const selected = signal("one");
  const tab = (value: string) => (
    <Tab selected={selected() === value} onPress={() => selected.set(value)} class="px-3 py-2">
      <Text>{`${props.name} ${value}`}</Text>
    </Tab>
  );
  return (
    <View style={{ gap: 8 }}>
      <Tabs
        orientation={props.orientation}
        accessibilityLabel={props.name}
        class={props.orientation === "vertical" ? "flex-col" : "flex-row"}
      >
        {tab("one")}
        {tab("two")}
        {tab("three")}
      </Tabs>
      <Text>{`${props.name} selected: ${selected()}`}</Text>
    </View>
  );
}

mount(() => (
  <View style={{ gap: 24, padding: 16 }}>
    <List name="Row" />
    <List name="Column" orientation="vertical" />
  </View>
));
