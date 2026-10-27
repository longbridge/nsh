import { colorScheme, type JSX, Scroll, Text, View } from "nsh";
import { createNavStack, NavPage } from "nsh/base";

/**
 * Sizes are in points, as on every platform; in the terminal a Cell is
 * 8 × 16 of them, so the layout keeps to whole Cells: 8 across, 16 down,
 * and a 1 pt border takes a whole row or column of Cells.
 *
 * The look is Emacs's (dired, magit, ibuffer): text first, at most one
 * Cell of padding, no filled blocks. Pages have no background, so the
 * terminal's own shows through; text colors follow its light or dark
 * appearance, and one accent marks what is selected or active.
 */
const DARK = {
  text: "#e6e8ee",
  muted: "#9aa1b2",
  faint: "#5f6676",
  line: "#3b4150",
  accent: "#7aa2f7",
  up: "#3fb950",
  down: "#f85149",
  warn: "#e3b341",
  violet: "#bb9af7",
  /** Only for what must hide the page under it (a dialog). */
  page: "#0f1115",
};

const LIGHT: typeof DARK = {
  text: "#1f2328",
  muted: "#57606a",
  faint: "#8c959f",
  line: "#d0d7de",
  accent: "#0969da",
  up: "#1a7f37",
  down: "#cf222e",
  warn: "#9a6700",
  violet: "#8250df",
  page: "#ffffff",
};

type Palette = typeof DARK;

/** The palette of the terminal's appearance, read reactively. */
export const colors = {} as Palette;
for (const key of Object.keys(DARK) as (keyof Palette)[]) {
  Object.defineProperty(colors, key, { enumerable: true, get: () => (colorScheme() === "light" ? LIGHT : DARK)[key] });
}

export interface Demo {
  name: string;
  detail: string;
  screen: () => JSX.Element;
}

export type Route = { name: "home" } | { name: "demo"; demo: Demo };

export const nav = createNavStack<Route>([{ name: "home" }]);

/** A horizontal separator: one row of ─. */
export function Rule() {
  return <View style={{ height: 16, borderBottomWidth: 1, borderColor: colors.line }} />;
}

/** A vertical separator: one column of │, as tall as its row. */
export function VerticalRule() {
  return <View style={{ width: 8, borderLeftWidth: 1, borderColor: colors.line }} />;
}

/** A header line, as Emacs's: the back button, the title, a detail; a rule below. */
export function Header(props: { title: string; detail?: string; back?: boolean }) {
  return (
    <View>
      <View style={{ height: 16, flexDirection: "row", gap: 16, paddingHorizontal: 8 }}>
        <If when={props.back}>
          <Button label="Back" onPress={() => nav.pop()} />
        </If>
        <Text style={{ fontWeight: "700", color: colors.text }}>{props.title}</Text>
        <Text style={{ numberOfLines: 1, flex: 1, color: colors.muted }}>
          {props.detail ?? ""}
        </Text>
      </View>
      <Rule />
    </View>
  );
}

/** A page's content, one Cell in from the edges, scrolling. */
export function Page(props: { children: JSX.Element }) {
  return (
    <Scroll style={{ flex: 1 }}>
      <View style={{ paddingHorizontal: 8, paddingVertical: 16, gap: 16 }}>{props.children}</View>
    </Scroll>
  );
}

/** A demo's page: its header line, its content below. */
export function DemoPage(props: { demo: Demo; children: JSX.Element }) {
  return (
    <NavPage title={props.demo.name} header={<Header title={props.demo.name} detail={props.demo.detail} back />}>
      <Page>{props.children}</Page>
    </NavPage>
  );
}

/** A button as text, `[ OK ]`: the accent and bold while active. */
export function Button(props: { label: string; onPress: () => void; tone?: string; active?: boolean }) {
  return (
    <View onPress={props.onPress} role="button" accessibilityLabel={props.label} style={{ height: 16 }}>
      <Text style={{ fontWeight: props.active ? "700" : "400", color: props.active ? colors.accent : (props.tone ?? colors.text) }}>
        {`[ ${props.label} ]`}
      </Text>
    </View>
  );
}

/** A section, as magit's: a bold heading, its rows right below. */
export function Section(props: { title?: string; children: JSX.Element; grow?: boolean }) {
  return (
    <View style={{ flex: props.grow ? 1 : undefined }}>
      <If when={props.title}>
        <Text style={{ fontWeight: "700", color: colors.text }}>{props.title}</Text>
      </If>
      {props.children}
    </View>
  );
}

/** A label and a value, top-aligned so a value can grow rows below. */
export function Field(props: { label: string; children: JSX.Element }) {
  return (
    <View style={{ flexDirection: "row", gap: 16 }}>
      <Text style={{ width: 128, color: colors.muted }}>{props.label}</Text>
      {props.children}
    </View>
  );
}
