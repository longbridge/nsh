import { mount, Scroll, signal, Text, View, type YNode } from "nsh";
import { Platform } from "nsh/device";
import { ui } from "./palette";

// The page colors, read when they render: they follow the theme.
const colors = (): string[] => [ui().accent, ui().info, ui().violet, ui().down, ui().warn, ui().up];

function App() {
  const page = signal(0);
  // A page is as wide as the scroll view: 390 on a phone, wider on a desktop.
  const width = signal(390);
  const y = signal(0);
  let scroller!: YNode;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text, paddingHorizontal: 20 }}>scroll</Text>

      <Text style={{ color: ui().muted, paddingHorizontal: 20 }}>
        Horizontal, pagingEnabled · page {page() + 1} · {Platform.isDesktop ? "Shift+wheel or swipe the trackpad" : "swipe"}
      </Text>
      <Scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 140, flexGrow: 0 }}
        onLayout={(e) => width.set(e.width)}
        onMomentumScrollEnd={(e) => page.set(Math.round(e.x / width()))}
      >
        <Each of={colors()}>
          {(color, index) => (
            <View style={{ width: width(), padding: 12 }}>
              <View style={{ flex: 1, borderRadius: 16, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <Text style={{ color: ui().page, fontSize: 32, fontWeight: "700" }}>{index() + 1}</Text>
              </View>
            </View>
          )}
        </Each>
      </Scroll>

      <View style={{ width: "100%", maxWidth: 640, alignSelf: "center", flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingHorizontal: 20 }}>
        <Text style={{ color: ui().muted }}>Vertical · y = {Math.round(y())}</Text>
        <View
          onPress={() => scroller.scrollTo({ y: 0, animated: true })}
          activeStyle={{ opacity: 0.6 }}
          style={{ ...ui().raised, borderRadius: 10, paddingHorizontal: 12, paddingVertical: 6 }}
        >
          <Text style={{ color: ui().text }}>scrollTo top</Text>
        </View>
      </View>
      <Scroll ref={scroller} style={{ flex: 1 }} scrollEventThrottle={16} onScroll={(e) => y.set(e.y)}>
        <View style={{ width: "100%", maxWidth: 640, alignSelf: "center", paddingHorizontal: 20, paddingBottom: 20, gap: 8 }}>
          <Each of={Array.from({ length: 40 }, (_, i) => i + 1)}>
            {(n) => (
              <View style={{ ...ui().row, borderRadius: 12, padding: 14 }}>
                <Text style={{ color: ui().text }}>Row {n()}</Text>
              </View>
            )}
          </Each>
        </View>
      </Scroll>
    </View>
  );
}

mount(App);
