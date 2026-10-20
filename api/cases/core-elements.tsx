// A case of the API's test suite (docs/dev/compatibility.md): the host
// elements of the primitives (`Scroll`, `Input`, `Image`, `Video`,
// `WebView`) and a native component nothing registered, each laid out where
// its style puts it; `nativeModule` reading a UI set the engine has and one
// it has not; `PanResponder`'s handlers spread on a view. Kept with
// `cargo xtask api --keep api/cases/core-elements.tsx`; its script checks
// each.
import { Input, Scroll, Text, View, mount } from "nsh";
import { PanResponder } from "nsh/gesture";
import { Image } from "nsh/image";
import { nativeComponent, nativeModule } from "nsh/native";
import { Video } from "nsh/video";
import { WebView } from "nsh/webview";

const Map = nativeComponent<{ zoom: number }, { regionChange: { zoom: number } }>("CompatMap", {
  props: ["zoom"],
  events: ["regionChange"],
});

function missingModule(): string {
  try {
    nativeModule<object>("host/compat-missing");
    return "found";
  } catch (e) {
    return e instanceof ReferenceError && String(e.message).includes("host/compat-missing")
      ? "ReferenceError"
      : `other: ${e}`;
  }
}

function App() {
  const mobile = nativeModule<Record<string, unknown>>("nsh/ui/mobile");
  const pan = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onPanResponderMove: () => {},
  });
  return (
    <View testID="root" style={{ flex: 1, paddingTop: 64 }}>
      <Scroll testID="scroll" style={{ height: 100 }}>
        <View testID="tall" style={{ height: 300 }}>
          <Text>In scroll</Text>
        </View>
      </Scroll>
      <Input testID="input" value="typed" placeholder="Name" style={{ width: 200, height: 36 }} />
      <Image testID="image" src={{ uri: "https://example.com/compat.png" }} style={{ width: 64, height: 48 }} />
      <Video testID="video" source="https://example.com/compat.mp4" paused style={{ width: 160, height: 90 }} />
      <WebView testID="web" source={{ html: "<p>compat</p>" }} style={{ width: 180, height: 60 }} />
      <Map testID="map" zoom={3} style={{ width: 100, height: 50 }} />
      <View testID="pan" {...pan.panHandlers} style={{ width: 50, height: 50 }} />
      <Text>Missing module: {missingModule()}</Text>
      <Text>Mobile module: {typeof mobile === "object" && mobile !== null ? "object" : typeof mobile}</Text>
      <Text>Same object: {String(nativeModule("nsh/ui/mobile") === mobile)}</Text>
      <Text>Pan handlers: {Object.keys(pan.panHandlers).length > 0 ? "some" : "none"}</Text>
      <Text>Pan start: {String(typeof pan.panHandlers.onStartShouldSetResponder === "function")}</Text>
    </View>
  );
}

mount(App);
