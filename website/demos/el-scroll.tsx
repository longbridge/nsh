import { mount, Scroll, signal, Text, View, type YNode } from "nsh";
import { ui } from "./palette";

// The page colors, read when they render: they follow the theme.
const colors = (): string[] => [ui().accent, ui().info, ui().violet, ui().down, ui().warn, ui().up];

function App() {
  const page = signal(0);
  const y = signal(0);
  let scroller!: YNode;
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text, paddingHorizontal: 20 }}>scroll</Text>

      <Text style={{ color: ui().muted, paddingHorizontal: 20 }}>Horizontal, pagingEnabled · page {page() + 1}</Text>
      <Scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 140, flexGrow: 0 }}
        onMomentumScrollEnd={(e) => page.set(Math.round(e.x / 390))}
      >
        <Each of={colors()}>
          {(color, index) => (
            <View style={{ width: 390, padding: 12 }}>
              <View style={{ flex: 1, borderRadius: 16, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <Text style={{ color: ui().page, fontSize: 32, fontWeight: "700" }}>{index() + 1}</Text>
              </View>
            </View>
          )}
        </Each>
      </Scroll>

      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingHorizontal: 20 }}>
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
        <View style={{ paddingHorizontal: 20, paddingBottom: 20, gap: 8 }}>
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
