// A case of the API's test suite (docs/dev/compatibility.md): what a scroll
// view reports as the user scrolls it: each offset with its content's and
// its own size and zoom, a drag beginning and ending, a fling's momentum
// beginning and ending, and the scroll coming to rest. Each handler shows
// what it gets as text. Kept with
// `cargo xtask api --keep api/cases/events-scroll.tsx`; its script checks
// each.
import { Scroll, Text, View, mount, signal } from "nsh";

type At = { x: number; y: number };

function App() {
  const scroll = signal("none");
  const steps = signal<string[]>([]);
  const step = (name: string) => (e: At) => steps.update((s) => [...s, `${name} ${e.x},${e.y}`]);
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>Scroll: {scroll()}</Text>
      <Text>Steps: {steps().join(" | ")}</Text>
      <Scroll
        testID="scroll"
        style={{ width: 300, height: 200 }}
        onScroll={(e) =>
          scroll.set(
            `${e.x},${e.y} content ${e.contentWidth}x${e.contentHeight} ` +
              `size ${e.width}x${e.height} zoom ${e.zoomScale}`,
          )
        }
        onScrollBeginDrag={step("begin")}
        onScrollEndDrag={step("end")}
        onMomentumScrollBegin={step("glide")}
        onMomentumScrollEnd={step("stop")}
        onScrollEnd={step("rest")}
      >
        <View style={{ height: 1000 }} />
      </Scroll>
    </View>
  );
}

mount(App);
