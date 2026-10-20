// A case of the API's test suite (docs/dev/compatibility.md): the layout
// components of `nsh/ui/mobile` placing the app's content (`Card`, `Group`,
// `ListView`, `SafeAreaView`, `KeyboardAvoidingView`), the insets and the
// keyboard as the headless runtime reports them (none, hidden), and the
// status bar and keyboard functions going through. Kept with
// `cargo xtask api --keep api/cases/mod-ui-layout.tsx`; its script checks
// each.
import { Text, View, mount, signal } from "nsh";
import {
  Card,
  Group,
  Keyboard,
  KeyboardAvoidingView,
  ListView,
  SafeAreaView,
  StatusBar,
  keyboard,
  safeArea,
  useSafeAreaInsets,
} from "nsh/ui/mobile";

const rows = ["Row one", "Row two", "Row three"];

function Insets() {
  const insets = useSafeAreaInsets();
  return <Text>{`Insets here: ${insets().top} ${insets().right} ${insets().bottom} ${insets().left}`}</Text>;
}

function App() {
  const calls = signal("not called");
  return (
    <SafeAreaView testID="safe" edges={["top", "bottom"]} style={{ flex: 1, paddingTop: 64 }}>
      <StatusBar barStyle="light-content" hidden={false} animated android={{ backgroundColor: "#000000" }} />
      <Text>{`Safe area: ${safeArea().top} ${safeArea().right} ${safeArea().bottom} ${safeArea().left}`}</Text>
      <Insets />
      <Text>{`Keyboard: ${keyboard().height}`}</Text>
      <Text>{`Calls: ${calls()}`}</Text>
      <View
        testID="call"
        onPress={() => {
          StatusBar.setBarStyle("dark-content", true);
          StatusBar.setHidden(true, "fade");
          StatusBar.setHidden(false);
          Keyboard.dismiss();
          calls.set("done");
        }}
      >
        <Text>Call them</Text>
      </View>
      <Card variant="outlined" header={<Text>Card header</Text>} footer={<Text>Card footer</Text>}>
        <Text>Card body</Text>
      </Card>
      <Card>
        <Text>Elevated card</Text>
      </Card>
      <Group header={<Text>Group header</Text>} footer={<Text>Group footer</Text>}>
        <Text>Group row one</Text>
        <Text>Group row two</Text>
      </Group>
      <View style={{ height: 160 }}>
        <ListView variant="inset" dividers each={rows}>
          {(row) => <Text>{row()}</Text>}
        </ListView>
      </View>
      <KeyboardAvoidingView testID="avoid" behavior="padding" keyboardVerticalOffset={20} style={{ height: 50 }}>
        <View testID="inside" style={{ height: 30 }} />
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

mount(App);
