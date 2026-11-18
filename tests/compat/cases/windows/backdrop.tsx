import { Text, View, type WindowsBackdrop, signal } from "nsh";

export default function Backdrop() {
  const backdrop = signal<WindowsBackdrop | undefined>("blur");
  return (
    <View style={{ paddingTop: 64 }}>
      <Text>Blur backdrop</Text>
      <View
        testID="backdrop-surface"
        windows={backdrop() === "blur" ? { backdrop: "blur" } : {}}
        macos={{ backdrop: { material: "hudWindow" } }}
        style={{ width: 120, height: 32, borderRadius: 12, backgroundColor: "#ffffff66" }}
      />
      <View testID="backdrop-clear" onPress={() => backdrop.set(undefined)}><Text>Clear backdrop</Text></View>
      <View testID="backdrop-set" onPress={() => backdrop.set("blur")}><Text>Set backdrop</Text></View>
    </View>
  );
}
