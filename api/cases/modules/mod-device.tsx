// A case of the API's test suite (docs/dev/compatibility.md): `nsh/device`
// as the headless runtime reports it (iOS, a phone, 1 pixel per point, the
// script's viewport), the color scheme an app sets, and right-to-left
// layout an app forces. Its script checks each.
import { Text, View } from "nsh";
import { I18nManager, PixelRatio, Platform, colorScheme, fontScale, setColorScheme, windowSize } from "nsh/device";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`OS: ${Platform.OS}`}</Text>
      <Text>{`Idiom: ${Platform.constants.interfaceIdiom}`}</Text>
      <Text>{`Pad: ${Platform.isPad} TV: ${Platform.isTV} Catalyst: ${Platform.isMacCatalyst} Desktop: ${Platform.isDesktop}`}</Text>
      <Text>{`Version: ${Platform.Version}`}</Text>
      <Text>{`Select: ${Platform.select({ ios: "ios", android: "android", default: "other" })}`}</Text>
      <Text>{`Select mobile: ${Platform.select({ mobile: "mobile", desktop: "desktop" })}`}</Text>
      <Text>{`Ratio: ${PixelRatio.get()} font ${PixelRatio.getFontScale()}`}</Text>
      <Text>{`Pixels: ${PixelRatio.getPixelSizeForLayoutSize(10)} round ${PixelRatio.roundToNearestPixel(10.4)}`}</Text>
      <Text>{`Font scale: ${fontScale()}`}</Text>
      <Text>{`Window: ${windowSize().width}x${windowSize().height}`}</Text>
      <Text>{`Scheme: ${colorScheme()}`}</Text>
      <Text>{`RTL: ${I18nManager.isRTL} swap ${I18nManager.doLeftAndRightSwapInRTL} constants ${I18nManager.getConstants().isRTL}`}</Text>
      <View testID="dark" onPress={() => setColorScheme("dark")}>
        <Text>Go dark</Text>
      </View>
      <View testID="system" onPress={() => setColorScheme("system")}>
        <Text>Follow the system</Text>
      </View>
      <View
        testID="rtl"
        onPress={() => {
          I18nManager.allowRTL(true);
          I18nManager.forceRTL(true);
        }}
      >
        <Text>Right to left</Text>
      </View>
      <View testID="ltr" onPress={() => I18nManager.forceRTL(false)}>
        <Text>Left to right</Text>
      </View>
      <View testID="row" style={{ flexDirection: "row", height: 20 }}>
        <View testID="first" style={{ width: 100, height: 20 }} />
      </View>
    </View>
  );
}

export default App;
