// The home page's Menu: one small file browser whose View menu is open when
// it starts, so each device's form shows at once. Every item acts on the
// browser behind it.
import { type JSX, mount, signal, Text, View } from "nsh";
import { Menu, MenuItem, MenuSeparator } from "nsh/base";
import { button, buttonText, desktop, MenuMark, menuIndicator, PopIn, tint, ui } from "./palette";

const FILES = [
  { name: ".env", hidden: true, modified: 6, size: 1 },
  { name: "App.tsx", hidden: false, modified: 1, size: 4 },
  { name: "Chart.tsx", hidden: false, modified: 3, size: 9 },
  { name: "Orders.tsx", hidden: false, modified: 2, size: 7 },
  { name: "nsh.json", hidden: false, modified: 5, size: 2 },
  { name: "theme.css", hidden: false, modified: 4, size: 3 },
];
const SORTS = ["Name", "Date modified", "Size"];

const sheet = () =>
  desktop ? undefined : { ...ui().card, borderTopLeftRadius: 20, borderTopRightRadius: 20, paddingHorizontal: 10, paddingVertical: 12 };
const backdrop = { backgroundColor: "rgba(0,0,0,0.45)" };

function Items(props: { children: JSX.Element }) {
  if (!desktop) return props.children;
  return <PopIn style={{ ...ui().card, borderRadius: 12, padding: 5, minWidth: 210 }}>{props.children}</PopIn>;
}

function Item(props: { label: string; onSelect?: () => void; checked?: boolean; radio?: boolean; keepOpen?: boolean }) {
  return (
    <MenuItem
      onSelect={props.onSelect}
      checked={props.checked}
      role={props.radio ? "menuitemradio" : undefined}
      indicatorClass={menuIndicator}
      indicator={props.checked === undefined ? undefined : <MenuMark kind={props.radio ? "dot" : "check"} tint={ui().accent} />}
      closeOnSelect={!props.keepOpen}
      hoverStyle={{ backgroundColor: tint(ui().accent, 0.08), borderColor: tint(ui().accent, 0.24) }}
      activeStyle={{ backgroundColor: tint(ui().accent, 0.14), borderColor: tint(ui().accent, 0.34) }}
      style={{ flexDirection: "row", alignItems: "center", gap: 8, height: desktop ? 32 : 46, paddingHorizontal: 10, borderRadius: 8, borderWidth: 1, borderColor: "transparent" }}
    >
      <Text style={{ numberOfLines: 1, flexGrow: 1, flexShrink: 0, fontSize: 14, color: ui().text }}>{props.label}</Text>
    </MenuItem>
  );
}

function Separator() {
  return <MenuSeparator style={{ height: 1, marginVertical: 4, marginHorizontal: 6, backgroundColor: ui().border }} />;
}

function Trigger(props: { label: string }) {
  return (
    <View {...button("secondary", { align: "flex-start" })}>
      <Text style={buttonText()}>{props.label} ▾</Text>
    </View>
  );
}

function App() {
  const sidebar = signal(true);
  const minimap = signal(false);
  const hidden = signal(false);
  const sort = signal("Name");
  const open = signal("App.tsx");
  const files = () =>
    FILES.filter((file) => hidden() || !file.hidden).sort((a, b) =>
      sort() === "Size" ? b.size - a.size : sort() === "Name" ? a.name.localeCompare(b.name) : a.modified - b.modified,
    );
  return (
    <View style={{ flex: 1, backgroundColor: ui().page }}>
      <View style={{ flexDirection: "row", gap: 8, padding: 12, borderBottomWidth: 1, borderColor: ui().border }}>
        <Menu offset={4} margin={8} popupStyle={sheet()} backdropStyle={backdrop} trigger={<Trigger label="File" />}>
          <Items>
            <Item label="New" />
            <Item label="Open…" />
            <Separator />
            <Item label="Save" />
          </Items>
        </Menu>
        <Menu defaultOpen offset={4} margin={8} popupStyle={sheet()} backdropStyle={backdrop} trigger={<Trigger label="View" />}>
          <Items>
            <Item label="Show sidebar" checked={sidebar()} keepOpen onSelect={() => sidebar.set(!sidebar())} />
            <Item label="Show minimap" checked={minimap()} keepOpen onSelect={() => minimap.set(!minimap())} />
            <Item label="Show hidden files" checked={hidden()} keepOpen onSelect={() => hidden.set(!hidden())} />
            <Separator />
            <Text style={{ fontSize: 12, fontWeight: "600", color: ui().faint, paddingHorizontal: 10, paddingTop: 6, paddingBottom: 2 }}>Sort by</Text>
            <Each of={SORTS}>{(by) => <Item label={by()} radio checked={sort() === by()} onSelect={() => sort.set(by())} />}</Each>
          </Items>
        </Menu>
      </View>
      <View style={{ flex: 1, flexDirection: "row" }}>
        <If when={sidebar()}>
          <View style={{ width: 132, padding: 8, gap: 2, borderRightWidth: 1, borderColor: ui().border }}>
            <Each of={files()} key={(file) => file.name}>
              {(file) => (
                <View
                  onPress={() => open.set(file().name)}
                  hoverStyle={{ backgroundColor: tint(ui().accent, 0.08) }}
                  style={{ paddingHorizontal: 8, paddingVertical: 5, borderRadius: 6, backgroundColor: open() === file().name ? tint(ui().accent, 0.14) : "transparent" }}
                >
                  <Text style={{ numberOfLines: 1, fontSize: 13, color: open() === file().name ? ui().text : ui().muted }}>{file().name}</Text>
                </View>
              )}
            </Each>
          </View>
        </If>
        <View style={{ flex: 1, padding: 16 }}>
          <Text style={{ fontSize: 15, fontWeight: "600", color: ui().text }}>{open()}</Text>
        </View>
        <If when={minimap()}>
          <View style={{ width: 44, padding: 8, gap: 5, borderLeftWidth: 1, borderColor: ui().border }}>
            <Each of={[22, 16, 26, 12, 20, 24, 14]}>{(width) => <View style={{ width: width(), height: 2, backgroundColor: ui().faint }} />}</Each>
          </View>
        </If>
      </View>
    </View>
  );
}

mount(App);
