// Navigation: a stack of pages with header options, pushed and popped.
import { type JSX, mount, signal, Text, View } from "nsh";
import { createNavStack, NavPage, NavStack } from "nsh/base";
import { ui } from "./palette";

function Button(props: { title: string; variant?: "primary" | "secondary"; onPress: () => void }) {
  const primary = () => props.variant !== "secondary";
  return (
    <View role="button" onPress={props.onPress} style={primary() ? ui().primary : ui().raised} class="items-center rounded-xl py-3 active:opacity-75">
      <Text style={{ fontWeight: "600", color: primary() ? ui().onPrimary : ui().text }}>{props.title}</Text>
    </View>
  );
}

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavStack<Route>([{ name: "home" }]);

// The page's own header where NSH draws none (the web, the desktops); iOS and
// Android show the system's, from `title` and `headerRight`.
function Header(props: { title: string; right?: JSX.Element }) {
  return (
    <View style={{ backgroundColor: ui().page, borderBottomWidth: 1, borderBottomColor: ui().line }} class="h-12 flex-row items-center px-2">
      <View class="w-20 flex-row">
        <If when={nav.routes().length > 1}>
          <View role="button" accessibilityLabel="Back" onPress={() => nav.pop()} class="px-2 py-1 active:opacity-60">
            <Text style={{ fontSize: 17, color: ui().accent }}>‹ Back</Text>
          </View>
        </If>
      </View>
      <Text numberOfLines={1} style={{ fontSize: 17, fontWeight: "600", color: ui().text }} class="flex-1 text-center">
        {props.title}
      </Text>
      <View class="w-20 flex-row justify-end px-2">{props.right}</View>
    </View>
  );
}

// A page's content: a column as wide as a phone, centered in a wider window.
function Body(props: { children: JSX.Element }) {
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 16, paddingTop: 24 }}>
      <View style={{ width: "100%", maxWidth: 480, alignSelf: "center", gap: 12 }}>{props.children}</View>
    </View>
  );
}

function Home() {
  const opened = signal(0);
  return (
    <NavPage title="Inbox" header={<Header title="Inbox" />}>
      <Body>
        <Text style={{ color: ui().muted }}>{`Opened ${opened()} messages`}</Text>
        <Button
          title="Open message 1"
          onPress={() => {
            opened.set(opened() + 1);
            nav.push({ name: "detail", id: 1 });
          }}
        />
        <Button title="Open message 2" variant="secondary" onPress={() => nav.push({ name: "detail", id: 2 })} />
      </Body>
    </NavPage>
  );
}

function Detail(props: { id: number }) {
  const starred = signal(false);
  const star = () => (
    <Text onPress={() => starred.set(!starred())} style={{ fontSize: 17, color: ui().accent }}>
      {starred() ? "★" : "☆"}
    </Text>
  );
  return (
    <NavPage title={`Message ${props.id}`} headerRight={star()} header={<Header title={`Message ${props.id}`} right={star()} />}>
      <Body>
        <Text style={{ color: ui().text }}>{`The body of message ${props.id}.`}</Text>
        <Button title="Next message" onPress={() => nav.push({ name: "detail", id: props.id + 1 })} />
        <Button title="Back to the inbox" variant="secondary" onPress={() => nav.popToRoot()} />
      </Body>
    </NavPage>
  );
}

function App() {
  return (
    <NavStack state={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </NavStack>
  );
}

mount(App);
