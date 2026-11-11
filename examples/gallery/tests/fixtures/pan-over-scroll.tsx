import { mount, platform, Scroll, signal, Text, View } from "nsh";

// A chat: a list filling the screen and a composer card over its bottom
// that a drag down moves (`panActivation` and responder moves); a chart in
// the list scrubs sideways.
function App() {
  const offset = signal(0);
  const card = signal(0);
  let startY = 0;
  return (
    <View style={{ flex: 1, paddingTop: 80 }}>
      <Text>{offset() > 0 ? "list: scrolled" : "list: at top"}</Text>
      <Text>{`card: ${card() > 0 ? "dragged" : "still"}`}</Text>
      <Text>{`Platform: ${platform.os}`}</Text>
      <View style={{ flex: 1 }}>
        <Scroll onScroll={(e) => offset.set(e.offset.y)} style={{ flex: 1 }}>
          <View
            testID="chart"
            panActivation={{ activeOffsetX: 6, failOffsetY: 8 }}
            onResponderMove={() => {}}
            style={{ height: 120, backgroundColor: "#e8eef6" }}
          />
          {Array.from({ length: 60 }, (_, i) => (
            <View style={{ height: 44, justifyContent: "center", paddingHorizontal: 16 }}>
              <Text>{`Message ${i + 1}`}</Text>
            </View>
          ))}
        </Scroll>
        <View
          testID="card"
          panActivation={{ activeOffsetY: 10, failOffsetX: 10 }}
          onResponderGrant={(e) => (startY = e.pageY)}
          onResponderMove={(e) => card.set(Math.abs(e.pageY - startY))}
          style={{ position: "absolute", left: 16, right: 16, bottom: 16, height: 160, backgroundColor: "#dddddd" }}
        >
          <Text>Composer</Text>
        </View>
      </View>
    </View>
  );
}

mount(App);
