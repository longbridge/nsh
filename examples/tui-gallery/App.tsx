import { mount, Scroll, Text, View } from "nsh";
import { NavPage, NavStack } from "nsh/base";
import { Controls, Counter, Form, Layout, LongList, Scope, TextDemo, Watchlist } from "./demos";
import { colors, type Demo, DemoPage, Header, nav } from "./ui";

/** The demos, by what they show: the gallery's index, for a terminal. */
const groups: { title: string; demos: Demo[] }[] = [
  {
    title: "Apps",
    demos: [
      { name: "Scope", detail: "An oscilloscope, a spectrum and a vectorscope drawn on canvases in braille dots", screen: Scope },
      { name: "Watchlist", detail: "Quotes ticking every 300 ms, sorting, a selected row, sparklines", screen: Watchlist },
      { name: "Counter", detail: "Signals, buttons, digits drawn with blocks", screen: Counter },
      { name: "Form", detail: "Inputs with focus, typing, validation, a checkbox", screen: Form },
    ],
  },
  {
    title: "Layout and controls",
    demos: [
      { name: "Controls", detail: "Switches, a segmented control, radios, progress, a dialog over a backdrop", screen: Controls },
      { name: "Layout", detail: "Flexbox: grow, justify, wrap; borders and corners", screen: Layout },
      { name: "Text", detail: "Styles, colors, wide characters, wrapping, truncation, alignment", screen: TextDemo },
      { name: "Long list", detail: "500 rows in a scroll view: the wheel, arrow and page keys", screen: LongList },
    ],
  },
];

function DemoRow(props: { demo: Demo; index: number }) {
  return (
    <View
      onPress={() => nav.push({ name: "demo", demo: props.demo })}
      role="button"
      accessibilityLabel={props.demo.name}
      style={{ flexDirection: "row", height: 16, gap: 16, paddingHorizontal: 8 }}
    >
      <Text style={{ width: 16, color: colors.faint }}>{String(props.index).padStart(2, "0")}</Text>
      <Text style={{ width: 128, fontWeight: "700", color: colors.text }}>{props.demo.name}</Text>
      <Text numberOfLines={1} style={{ flex: 1, color: colors.muted }}>
        {props.demo.detail}
      </Text>
      <Text style={{ color: colors.faint }}>›</Text>
    </View>
  );
}

const LOGO = [
  "█▄ █ █▀▀ █ █   ▀█▀ █ █ █",
  "█ ▀█ ▄▄█ █▀█    █  █▄█ █",
].join("\n");

function Home() {
  let index = 0;
  const numbered = groups.map((g) => ({ ...g, demos: g.demos.map((demo) => ({ demo, index: ++index })) }));
  return (
    <NavPage title="Gallery" header={<Header title="NSH Gallery" detail="for the terminal" />}>
      <Scroll style={{ flex: 1, backgroundColor: colors.background }}>
        <View style={{ paddingHorizontal: 16, paddingVertical: 16, gap: 16 }}>
          <View style={{ flexDirection: "row", gap: 32, alignItems: "center" }}>
            <Text style={{ color: colors.accent }}>{LOGO}</Text>
            <Text style={{ flex: 1, color: colors.muted }}>
              The same TSX, signals, flexbox and navigation as on iOS, Android, desktop and the web, drawn with
              cells. Click a demo, or Tab to it and press Enter; Esc goes back.
            </Text>
          </View>
          <Each of={numbered}>
            {(group) => (
              <View
                style={{
                  backgroundColor: colors.surface,
                  borderWidth: 1,
                  borderColor: colors.line,
                  borderRadius: 8,
                  paddingHorizontal: 8,
                  paddingVertical: 15,
                }}
              >
                <View style={{ position: "absolute", top: 0, left: 16, height: 16, paddingHorizontal: 8, backgroundColor: colors.surface }}>
                  <Text style={{ fontWeight: "700", color: colors.accent }}>{group().title.toUpperCase()}</Text>
                </View>
                <Each of={group().demos}>{(entry) => <DemoRow demo={entry().demo} index={entry().index} />}</Each>
              </View>
            )}
          </Each>
        </View>
      </Scroll>
    </NavPage>
  );
}

function App() {
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }}>
      <NavStack state={nav} style={{ flex: 1 }}>
        {(route) => {
          switch (route.name) {
            case "home":
              return <Home />;
            case "demo": {
              const Screen = route.demo.screen;
              return (
                <DemoPage demo={route.demo}>
                  <Screen />
                </DemoPage>
              );
            }
          }
        }}
      </NavStack>
    </View>
  );
}

mount(App);
