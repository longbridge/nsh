// A case of the API's test suite (docs/dev/compatibility.md): a `Stack` of
// `nsh/ui/mobile` driven by its navigator (`createNavigator`): screens
// pushed, replaced, popped, reset and popped to the top, each a `Screen`
// with header options, the routes read back. Kept with
// `cargo xtask api --keep api/cases/mod-ui-navigation.tsx`; its script
// checks each.
import { Text, View, mount } from "nsh";
import { Button, Screen, Stack, createNavigator } from "nsh/ui/mobile";

type Route = { name: string };

const home: Route = { name: "home" };
const nav = createNavigator<Route>(home);

function Page(props: { route: Route; index: number }) {
  return (
    <Screen title={`Title ${props.route.name}`} headerRight={<Text>Right</Text>}>
      <View style={{ flex: 1, paddingTop: 120, gap: 4 }}>
        <Text>{`Page ${props.route.name} at ${props.index}`}</Text>
        <Text>{`Routes: ${nav.routes().map((r) => r.name).join(" > ")}`}</Text>
        <Button title={`Push from ${props.route.name}`} onPress={() => nav.push({ name: `p${nav.routes().length}` })} />
        <Button title={`Pop from ${props.route.name}`} onPress={() => nav.pop()} />
        <Button title={`Replace ${props.route.name}`} onPress={() => nav.replace({ name: "swapped" })} />
        <Button title={`Reset from ${props.route.name}`} onPress={() => nav.reset([home, { name: "a" }, { name: "b" }])} />
        <Button title={`Top from ${props.route.name}`} onPress={() => nav.popToTop()} />
        <Button title={`Remove below ${props.route.name}`} onPress={() => nav.remove(nav.routes()[1])} />
      </View>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route, index) => <Page route={route} index={index()} />}
    </Stack>
  );
}

mount(App);
