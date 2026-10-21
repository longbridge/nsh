// A case of the API's test suite (docs/dev/compatibility.md): the safe
// areas and the software keyboard of `nsh/base`. `safeArea()` and
// `safeAreaInsets()` at the root (the surface's, none headless), inside a
// `NavPage` and inside a `Dialog` (theirs, as the platform reports them);
// `SafeAreaView` padding by them on the edges it is given, on top of its
// own padding; `keyboardFrame()` (no keyboard headless), `Keyboard.dismiss`
// and `KeyboardAvoidingView`, which makes no room while no keyboard shows,
// whatever its `behavior` and `offset`. Its
// script checks each.
import { Text, View, signal } from "nsh";
import {
  Button,
  Dialog,
  type Edges,
  Keyboard,
  KeyboardAvoidingView,
  NavPage,
  NavStack,
  SafeAreaView,
  createNavStack,
  keyboardFrame,
  safeArea,
  safeAreaInsets,
} from "nsh/base";

const show = (i: Edges) => `${i.top} ${i.right} ${i.bottom} ${i.left}`;

function Here(props: { name: string }) {
  const insets = safeAreaInsets();
  return <Text>{`${props.name} insets: ${show(insets())}`}</Text>;
}

const nav = createNavStack(["page"]);

function App() {
  const dismissed = signal("not called");
  return (
    <View style={{ flex: 1 }}>
      <Text>{`Safe area: ${show(safeArea())}`}</Text>
      <Here name="Root" />
      <Text>{`Keyboard: ${keyboardFrame().height}, duration a ${typeof keyboardFrame().duration}`}</Text>
      <Text>{`Dismiss: ${dismissed()}`}</Text>
      <Button
        onPress={() => {
          Keyboard.dismiss();
          dismissed.set("called");
        }}
      >
        <Text>Dismiss the keyboard</Text>
      </Button>
      <SafeAreaView testID="root-safe" style={{ height: 40, padding: 2 }}>
        <View testID="root-inner" style={{ flex: 1 }} />
      </SafeAreaView>
      <KeyboardAvoidingView testID="avoid-padding" offset={20} style={{ height: 30 }}>
        <View testID="avoid-padding-inner" style={{ flex: 1 }} />
      </KeyboardAvoidingView>
      <KeyboardAvoidingView testID="avoid-height" behavior="height" offset={20} style={{ height: 30 }}>
        <View testID="avoid-height-inner" style={{ flex: 1 }} />
      </KeyboardAvoidingView>
      <KeyboardAvoidingView testID="avoid-off" enabled={false} behavior="padding" style={{ height: 30 }}>
        <View testID="avoid-off-inner" style={{ flex: 1 }} />
      </KeyboardAvoidingView>
      <NavStack state={nav} style={{ height: 300 }}>
        {() => (
          <NavPage title="Insets">
            <View style={{ flex: 1 }}>
              <Here name="Page" />
              <SafeAreaView testID="page-all" style={{ height: 100, padding: 2 }}>
                <View testID="page-all-inner" style={{ flex: 1 }} />
              </SafeAreaView>
              <SafeAreaView testID="page-vertical" edges={["top", "bottom"]} style={{ height: 100 }}>
                <View testID="page-vertical-inner" style={{ flex: 1 }} />
              </SafeAreaView>
              <SafeAreaView testID="page-left" edges={["left"]} style={{ height: 20 }}>
                <View testID="page-left-inner" style={{ flex: 1 }} />
              </SafeAreaView>
            </View>
          </NavPage>
        )}
      </NavStack>
      <Dialog>
        <Here name="Dialog" />
        <SafeAreaView testID="dialog-safe" edges={["bottom", "right"]} style={{ height: 100 }}>
          <View testID="dialog-inner" style={{ flex: 1 }} />
        </SafeAreaView>
      </Dialog>
    </View>
  );
}

export default App;
