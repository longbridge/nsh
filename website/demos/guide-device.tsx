import { mount, Text, View } from "nsh";
import { colorScheme, fontScale, I18nManager, PixelRatio, Platform, windowSize } from "nsh/device";

function Row(props: { label: string; value: string }) {
  return (
    <View class="flex-row py-2 border-b border-[#e9e9e4] dark:border-[#1f2124]">
      <Text class="flex-1 text-[#565a61] dark:text-[#8d9198]">{props.label}</Text>
      <Text class="text-[#15161a] dark:text-[#eceef1]">{props.value}</Text>
    </View>
  );
}

function App() {
  const hairline = 1 / PixelRatio.get();
  return (
    <View class="flex-1 bg-[#f3f3ef] dark:bg-[#101113] px-4 pt-6 gap-4">
      <Text class="text-2xl font-bold text-[#15161a] dark:text-[#eceef1]">Environment</Text>
      <View class="rounded-2xl bg-white border border-[#e1e1db] dark:bg-[#18191c] dark:border-[#2c2e33] px-4 py-1">
        <Row label="windowSize()" value={`${Math.round(windowSize().width)} × ${Math.round(windowSize().height)}`} />
        <Row label="colorScheme()" value={colorScheme()} />
        <Row label="fontScale()" value={fontScale().toFixed(2)} />
        <Row label="Platform.OS" value={Platform.OS} />
        <Row label="Platform.Version" value={String(Platform.Version)} />
        <Row label="interfaceIdiom" value={Platform.constants.interfaceIdiom} />
        <Row label="Locale" value={new Intl.DateTimeFormat().resolvedOptions().locale} />
        <Row label="PixelRatio.get()" value={String(PixelRatio.get())} />
        <Row label="I18nManager.isRTL" value={String(I18nManager.isRTL)} />
      </View>

      <Text class="text-[#565a61] dark:text-[#8d9198]">
        {Platform.select({ ios: "Running on iOS.", android: "Running on Android.", web: "Running in a browser.", default: "Running elsewhere." })}
      </Text>

      <View class="gap-1">
        <View class="bg-[#8c9097] dark:bg-[#5a5e65]" style={{ height: hairline }} />
        <Text class="text-xs text-[#8c9097] dark:text-[#5a5e65]">The line above is one device pixel tall.</Text>
      </View>

      <View class={windowSize().width >= 600 ? "flex-row gap-3" : "gap-3"}>
        <View class="flex-1 rounded-xl bg-[#4a6a0a] dark:bg-[#c8f26d] p-4">
          <Text class="text-white dark:text-[#13160c] font-semibold">dark: classes follow the system</Text>
        </View>
        <View class="flex-1 rounded-xl bg-white border border-[#e1e1db] dark:bg-[#24262a] dark:border-[#2c2e33] p-4">
          <Text class="text-[#15161a] dark:text-[#eceef1] font-semibold">A row only when the window is 600 pt or wider</Text>
        </View>
      </View>

      <View
        onPress={() => I18nManager.forceRTL(!I18nManager.isRTL)}
        activeOpacity={0.6}
        class="flex-row items-center gap-3 rounded-2xl bg-white border border-[#e1e1db] dark:bg-[#18191c] dark:border-[#2c2e33] p-4"
      >
        <Text class="text-lg text-[#4a6a0a] dark:text-[#c8f26d]">⇄</Text>
        <Text class="flex-1 text-[#15161a] dark:text-[#eceef1]">Lay out right to left: {I18nManager.isRTL ? "on" : "off"}</Text>
      </View>
    </View>
  );
}

mount(App);
