import { mount, Text, View } from "nsh";
import { NavPage, NavStack } from "nsh/base";
import { Controls, Counter, Form, Layout, LongList, Scope, TextDemo, Watchlist } from "./demos";
import { colors, type Demo, DemoPage, Header, nav, Page, Section } from "./ui";

/** The demos, by what they show: the gallery's index, as a dired-like listing. */
const groups: { title: string; demos: Demo[] }[] = [
  {
    title: "Apps",
    demos: [
      { name: "Scope", detail: "An oscilloscope, a spectrum and a vectorscope drawn on canvases in braille dots", screen: Scope },
      { name: "Watchlist", detail: "Quotes ticking every 300 ms, sorting, a selected row, sparklines", screen: Watchlist },
      { name: "Counter", detail: "Signals, text buttons, digits drawn with blocks", screen: Counter },
      { name: "Form", detail: "Inputs with focus, typing, validation, a checkbox", screen: Form },
    ],
  },
  {
    title: "Layout and controls",
    demos: [
      { name: "Controls", detail: "Switches, a segmented control, radios, progress, a dialog over the page", screen: Controls },
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
      style={{ flexDirection: "row", height: 16, gap: 16, borderLeftWidth: 8, borderColor: "transparent" }}
      focusStyle={{ backgroundColor: colors.focus, borderColor: colors.accent }}
    >
      <Text style={{ width: 16, color: colors.faint }}>{String(props.index).padStart(2, "0")}</Text>
      <Text style={{ width: 128, color: colors.text }}>{props.demo.name}</Text>
      <Text style={{ numberOfLines: 1, flex: 1, color: colors.muted }}>
        {props.demo.detail}
      </Text>
    </View>
  );
}

function Home() {
  let index = 0;
  const numbered = groups.map((g) => ({ ...g, demos: g.demos.map((demo) => ({ demo, index: ++index })) }));
  return (
    <NavPage title="Gallery" header={<Header title="NSH Gallery" detail="for the terminal" />}>
      <Page>
        <Text style={{ color: colors.muted }}>
          The same TSX, signals, flexbox and navigation as on iOS, Android, desktop and the web, drawn with Cells. Use Tab /
          Shift+Tab, C-n / C-p or ↓ / ↑ to move between demos. Enter or a click opens one; Esc or C-g goes back. C-c quits.
        </Text>
        <Each of={numbered}>
          {(group) => (
            <Section title={`${group().title} (${group().demos.length})`}>
              <Each of={group().demos}>{(entry) => <DemoRow demo={entry().demo} index={entry().index} />}</Each>
            </Section>
          )}
        </Each>
      </Page>
    </NavPage>
  );
}

function App() {
  return (
    <View style={{ flex: 1 }}>
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
