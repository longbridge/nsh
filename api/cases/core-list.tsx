// A case of the API's test suite (docs/dev/compatibility.md): `nsh/list`'s
// `List` (`nsh:rt`'s `list`): the rows near the viewport rendered from
// `each`, with a header, and rows added when `each` changes. Kept with
// `cargo xtask api --keep api/cases/core-list.tsx`; its script checks each.
import { Text, View, mount, signal } from "nsh";
import { List } from "nsh/list";

function App() {
  const items = signal(["Apple", "Banana", "Cherry"]);
  return (
    <View testID="root" style={{ flex: 1, paddingTop: 64 }}>
      <View testID="add" onPress={() => items.set([...items(), `Item ${items().length + 1}`])}>
        <Text>Add</Text>
      </View>
      <List testID="list" each={items()} style={{ height: 400 }} ListHeaderComponent={<Text>Fruits</Text>}>
        {(item, index) => (
          <View style={{ height: 40 }}>
            <Text>
              {index() + 1}. {item()}
            </Text>
          </View>
        )}
      </List>
    </View>
  );
}

mount(App);
