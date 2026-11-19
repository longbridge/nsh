import { mount, signal, Text, View, type ViewRef } from "nsh";
import { TextView } from "nsh/base";

// Two selectable views, each reporting what is selected in it, and a
// text outside them.
function App() {
  let article!: ViewRef;
  const selected = signal("");
  const changes = signal(0);
  const heard = ({ text }: { text: string }) => {
    selected.set(text);
    changes.set(changes() + 1);
  };
  return (
    <View style={{ flex: 1, padding: 16, gap: 16, alignItems: "flex-start" }}>
      <View ref={article} testID="article" selectable onSelectionChange={heard} style={{ alignItems: "flex-start" }}>
        <Text testID="first">Alpha beta</Text>
        <Text testID="second">Gamma delta</Text>
      </View>
      <TextView testID="notes" selectable onSelectionChange={heard} markdown={"- one\n- two"} />
      <Text testID="outside">Elsewhere</Text>
      <View testID="clear" onPress={() => article.clearSelection()}>
        <Text>Clear</Text>
      </View>
      <Text>{`Selected: ${JSON.stringify(selected())}`}</Text>
      <Text>{`Changes: ${changes()}`}</Text>
    </View>
  );
}

mount(App);
