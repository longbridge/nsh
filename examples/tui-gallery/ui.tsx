import { type JSX, Scroll, Text, View } from "nsh";
import { createNavStack, NavPage } from "nsh/base";

/**
 * Sizes are in points, as on every platform; in the terminal a cell is
 * 8 × 16 of them, so the layout keeps to multiples of 8 across and 16
 * down: a 1 pt border takes a whole row or column of cells.
 */
export const colors = {
  background: "#0f1115",
  surface: "#161920",
  raised: "#1f232d",
  line: "#2b303b",
  text: "#e6e8ee",
  muted: "#9aa1b2",
  faint: "#5f6676",
  accent: "#7aa2f7",
  up: "#3fb950",
  down: "#f85149",
  warn: "#e3b341",
  violet: "#bb9af7",
};

export interface Demo {
  name: string;
  detail: string;
  screen: () => JSX.Element;
}

export type Route = { name: "home" } | { name: "demo"; demo: Demo };

export const nav = createNavStack<Route>([{ name: "home" }]);

/** A bar three rows tall: the back button, the title, what goes right. */
export function Header(props: { title: string; detail?: string; back?: boolean; right?: JSX.Element }) {
  return (
    <View
      style={{
        height: 48,
        flexDirection: "row",
        alignItems: "center",
        gap: 16,
        paddingHorizontal: 16,
        backgroundColor: colors.surface,
        borderBottomWidth: 1,
        borderBottomColor: colors.line,
      }}
    >
      <If when={props.back}>
        <Button label="‹ Back" onPress={() => nav.pop()} />
      </If>
      <Text style={{ fontWeight: "700", color: colors.text }}>{props.title}</Text>
      <Text numberOfLines={1} style={{ flex: 1, color: colors.faint }}>
        {props.detail ?? ""}
      </Text>
      {props.right}
    </View>
  );
}

/** A demo's page: its header, its content scrolling below. */
export function DemoPage(props: { demo: Demo; children: JSX.Element }) {
  return (
    <NavPage title={props.demo.name} header={<Header title={props.demo.name} detail={props.demo.detail} back />}>
      <Scroll style={{ flex: 1, backgroundColor: colors.background }}>
        <View style={{ paddingHorizontal: 16, paddingVertical: 16, gap: 16 }}>{props.children}</View>
      </Scroll>
    </NavPage>
  );
}

/** A one-row button: a filled label. */
export function Button(props: { label: string; onPress: () => void; tone?: string; active?: boolean }) {
  const fill = () => (props.active ? (props.tone ?? colors.accent) : colors.raised);
  return (
    <View
      onPress={props.onPress}
      role="button"
      accessibilityLabel={props.label}
      style={{ height: 16, paddingHorizontal: 8, backgroundColor: fill() }}
    >
      <Text style={{ fontWeight: "600", color: props.active ? colors.background : (props.tone ?? colors.text) }}>
        {props.label}
      </Text>
    </View>
  );
}

/** A boxed section with a title in its top border's row. */
export function Card(props: { title?: string; children: JSX.Element; grow?: boolean }) {
  return (
    <View
      style={{
        flex: props.grow ? 1 : undefined,
        backgroundColor: colors.surface,
        borderWidth: 1,
        borderColor: colors.line,
        borderRadius: 8,
        paddingHorizontal: 16,
        paddingVertical: 15,
        gap: 16,
      }}
    >
      <If when={props.title}>
        <View style={{ position: "absolute", top: 0, left: 16, height: 16, paddingHorizontal: 8, backgroundColor: colors.surface }}>
          <Text style={{ fontWeight: "700", color: colors.accent }}>{props.title}</Text>
        </View>
      </If>
      {props.children}
    </View>
  );
}

/** A label and a value on one row. */
export function Field(props: { label: string; children: JSX.Element }) {
  return (
    <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
      <Text style={{ width: 128, color: colors.muted }}>{props.label}</Text>
      {props.children}
    </View>
  );
}
