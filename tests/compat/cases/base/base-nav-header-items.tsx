// A case of the API's test suite (docs/dev/compatibility.md): trailing
// iOS bar buttons (`ios.headerRightItems`) with a native menu, their
// presses and menu selections (`onHeaderItemPress`, `onHeaderMenuSelect`).
// Other platforms ignore the iOS items and keep `headerRight`. The script
// checks the props reach the page and the events report the item / action
// ids.
import { Text, View, signal } from "nsh";
import { NavPage, NavStack, createNavStack } from "nsh/base";

const nav = createNavStack<string>(["home"]);
const log = signal("none");
const filter = signal("all");

function Home() {
  return (
    <NavPage
      {...{ testID: "home" }}
      title="Header items"
      headerRight={<Text>Fallback right</Text>}
      ios={{
        headerRightItems: [
          { id: "edit", systemItem: "edit", label: "Edit" },
          {
            id: "filter",
            title: "Filter",
            menu: {
              items: [
                { id: "all", title: "All", checked: filter() === "all" },
                { id: "open", title: "Open", checked: filter() === "open" },
                {
                  section: [{ id: "delete", title: "Delete", destructive: true }],
                },
              ],
            },
          },
          { id: "add", systemItem: "add" },
        ],
      }}
      onHeaderItemPress={({ id }) => log.set(`press ${id}`)}
      onHeaderMenuSelect={({ id }) => {
        if (id === "all" || id === "open") filter.set(id);
        log.set(`menu ${id}`);
      }}
    >
      <Text>{`Log: ${log()}`}</Text>
      <Text>{`Filter: ${filter()}`}</Text>
    </NavPage>
  );
}

function App() {
  return (
    <View style={{ flex: 1 }}>
      <NavStack state={nav} style={{ flex: 1 }}>
        {() => <Home />}
      </NavStack>
    </View>
  );
}

export default App;
