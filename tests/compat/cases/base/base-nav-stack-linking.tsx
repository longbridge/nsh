// A case of the API's test suite (docs/dev/compatibility.md): a `NavStack`
// with `linking`, which on the web makes each page a URL of the site
// (`path`) and opens the pages a URL names (`routes`). Off the web the
// stack keeps its own routes: it starts at its first route, pushes and
// pops as any stack, and never asks `linking`. Its script checks that.
import { Text, View, signal } from "nsh";
import { Button, NavPage, NavStack, createNavStack } from "nsh/base";

type Route = { name: "home" } | { name: "note"; slug: string };

const nav = createNavStack<Route>([{ name: "home" }]);
const asked = signal(0);

const linking = {
  path: (route: Route) => {
    asked.set(asked() + 1);
    return route.name === "note" ? `/notes/${route.slug}` : "/";
  },
  routes: (path: string): Route[] | null => {
    asked.set(asked() + 1);
    const match = /^\/notes\/([\w-]+)$/.exec(path);
    return match ? [{ name: "home" }, { name: "note", slug: match[1] }] : null;
  },
};

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 20 }}>
      <Text>{`Linked routes: ${nav.routes().map((r) => (r.name === "note" ? r.slug : r.name)).join(" > ")}`}</Text>
      <Text>{`Linking asked: ${asked()}`}</Text>
      <Button onPress={() => nav.push({ name: "note", slug: "q3" })}>
        <Text>Open q3</Text>
      </Button>
      <Button onPress={() => nav.pop()}>
        <Text>Back</Text>
      </Button>
      <NavStack state={nav} linking={linking} style={{ height: 200 }}>
        {(route) => (
          <NavPage title={route.name}>
            <Text>{route.name === "note" ? `Note ${route.slug}` : "Linked home"}</Text>
          </NavPage>
        )}
      </NavStack>
    </View>
  );
}

export default App;
