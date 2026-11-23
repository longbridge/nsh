// The same header as `TitleBarTabs.tsx` in nsh/base's `TitleBar`, with no
// padding of its own: the bar keeps its first tab clear of the traffic
// lights. `TitleBarTabs.test.tsx` runs it on the macOS host.
import { mount, signal, Text, View } from "nsh";
import { Button, TitleBar } from "nsh/base";
import { Window, currentWindow, titleBar } from "nsh/desktop";

const tabs = ["!1113", "!1114"];

function Tabs(props: { window: string; height: number }) {
  const window = currentWindow();
  window?.set({ titleBar: { style: "hidden", height: props.height } });
  const selected = signal("");
  const closed = signal<string[]>([]);
  return (
    <View style={{ flex: 1 }}>
      <TitleBar style={{ height: props.height, alignItems: "center", gap: 4 }}>
        {() =>
          tabs
            .filter((tab) => !closed().includes(tab))
            .map((tab) => (
              <View style={{ flexDirection: "row", alignItems: "center", height: 28 }}>
                <Button
                  accessibilityLabel={`Tab ${tab} ${props.window}`}
                  onPress={() => selected.set(tab)}
                  style={{ paddingHorizontal: 8, height: 28, justifyContent: "center" }}
                >
                  <Text>{tab}</Text>
                </Button>
                <Button
                  accessibilityLabel={`Close ${tab} ${props.window}`}
                  onPress={() => closed.set([...closed(), tab])}
                  style={{ width: 20, height: 28, justifyContent: "center" }}
                >
                  <Text>×</Text>
                </Button>
              </View>
            ))
        }
      </TitleBar>
      <Text>{() => `Selected ${props.window}: ${selected()}`}</Text>
      <Text>{() => `Closed ${props.window}: ${closed().join(",")}`}</Text>
      <Text>
        {() => {
          const bar = titleBar();
          return `Bar ${props.window}: ${Math.round(bar.height)} ${Math.round(bar.left)} ${Math.round(bar.right)}`;
        }}
      </Text>
      <Text>{() => `Mode ${props.window}: ${window?.state()?.mode ?? ""}`}</Text>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <Button
          accessibilityLabel={`Full screen ${props.window}`}
          onPress={() => window?.setMode("fullScreen")}
        >
          <Text>Full screen</Text>
        </Button>
        <Button
          accessibilityLabel={`Normal ${props.window}`}
          onPress={() => window?.setMode("normal")}
        >
          <Text>Normal</Text>
        </Button>
        {props.window === "main" ? (
          <Button
            accessibilityLabel="Open second"
            onPress={() =>
              Window.open(() => <Tabs window="second" height={60} />, {
                id: "second",
                title: "Second",
              })
            }
          >
            <Text>Open second</Text>
          </Button>
        ) : null}
      </View>
    </View>
  );
}

mount(() => <Window id="main">{() => <Tabs window="main" height={44} />}</Window>);
