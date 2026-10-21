import { Overlay, signal, Text, View, type YNode } from "nsh";
import { ContextMenu, Menu, MenuItem, MenuSeparator, Popover, Sheet, Submenu, Tooltip } from "nsh/base";
import { Platform } from "nsh/device";
import { Badge, Button, Caption, Card, colors, DemoPage, finish, mono, radius } from "../ui";

/** The popups' look: the gallery's, as `nsh/base` draws none. */
const popup = () => ({
  backgroundColor: colors.raised,
  borderRadius: radius.inset,
  borderWidth: 1,
  borderColor: colors.border,
  padding: 6,
  minWidth: 180,
  ...finish.raised,
});

/** A sheet's panel on touch screens: the bottom of the screen. */
const sheet = () => ({
  backgroundColor: colors.surface,
  borderTopLeftRadius: 20,
  borderTopRightRadius: 20,
  padding: 12,
  paddingBottom: 32,
  gap: 2,
});

const backdrop = { backgroundColor: "rgba(0,0,0,0.45)" };

/** A pressable chip that opens a popup. */
function Trigger(props: { label: string }) {
  return (
    <View
      activeStyle={{ backgroundColor: colors.border }}
      style={{ alignSelf: "flex-start", paddingHorizontal: 14, height: 36, justifyContent: "center", borderRadius: radius.control, backgroundColor: colors.raised, ...finish.raised }}
    >
      <Text style={{ fontSize: 14, fontWeight: "500", color: colors.text }}>{props.label}</Text>
    </View>
  );
}

/** An item of the demos' menus. */
function Item(props: { label: string; note?: string; onSelect?: () => void; disabled?: boolean }) {
  return (
    <MenuItem
      onSelect={props.onSelect}
      disabled={props.disabled}
      hoverStyle={{ backgroundColor: colors.border }}
      activeStyle={{ backgroundColor: colors.border }}
      disabledStyle={{ opacity: 0.4 }}
      style={{ flexDirection: "row", alignItems: "center", height: 36, paddingHorizontal: 10, borderRadius: 8 }}
    >
      <Text style={{ flex: 1, fontSize: 14, color: colors.text }}>{props.label}</Text>
      <If when={props.note}>
        <Text style={{ fontSize: 12, color: colors.faint, fontFamily: mono }}>{props.note}</Text>
      </If>
    </MenuItem>
  );
}

const separator = () => ({ height: 1, marginVertical: 4, backgroundColor: colors.line });

/**
 * Overlays: `<Overlay>` shows its children on top of the screen whatever
 * clips them, and `nsh/base`'s popups are built on it, each presented as
 * the device suits: popups and cascading menus on desktops, sheets on
 * phones.
 */
export function Overlays() {
  const last = signal("—");
  const device = Platform.isDesktop ? "desktop: popups, hover, right click" : "touch: sheets, held fingers";

  function Primitive() {
    const open = signal(false);
    let anchor: YNode | undefined;
    return (
      <Card icon="components" title="Overlay" subtitle="A box that clips its content, with an overlay anchored inside it" right={<Badge label="nsh" />}>
        <View style={{ height: 56, overflow: "hidden", borderRadius: radius.inset, backgroundColor: colors.inset, justifyContent: "center", paddingHorizontal: 12 }}>
          <View ref={anchor} onPress={() => open.set(!open())} style={{ alignSelf: "flex-start" }}>
            <Trigger label={open() ? "Close" : "Open the overlay"} />
          </View>
          <If when={open()}>
            <Overlay
              anchor={anchor}
              placement="bottom"
              align="start"
              offset={6}
              margin={8}
              onPressOutside={() => open.set(false)}
              onCancel={() => open.set(false)}
              onPosition={(e) => last.set(`overlay at ${e.placement}`)}
              style={popup()}
            >
              <Text style={{ fontSize: 14, color: colors.text, padding: 6 }}>Outside the clipped box, on top of the page.</Text>
            </Overlay>
          </If>
        </View>
        <Caption label="Scroll the page: it follows its anchor" />
      </Card>
    );
  }

  function Popovers() {
    return (
      <Card icon="components" title="Popover" subtitle="Anchored to its trigger on every device">
        <Popover
          offset={6}
          margin={8}
          popupStyle={popup()}
          onOpenChange={(open, reason) => last.set(`popover ${open ? "opened" : "closed"} (${reason})`)}
          trigger={<Trigger label="Show the popover" />}
        >
          <View style={{ padding: 8, gap: 4, maxWidth: 260 }}>
            <Text style={{ fontSize: 15, fontWeight: "600", color: colors.text }}>Popover</Text>
            <Text style={{ fontSize: 13, color: colors.muted }}>A press outside, Escape or back closes it.</Text>
          </View>
        </Popover>
      </Card>
    );
  }

  function Menus() {
    return (
      <Card icon="lists" title="Menu" subtitle={device}>
        <Menu
          offset={6}
          margin={8}
          popupStyle={Platform.isDesktop ? popup() : sheet()}
          backdropStyle={backdrop}
          onOpenChange={(open, reason) => reason !== "select" && last.set(`menu ${open ? "opened" : "closed"} (${reason})`)}
          trigger={<Trigger label="File" />}
        >
          <Item label="New" note="⌘N" onSelect={() => last.set("chose New")} />
          <Item label="Open…" note="⌘O" onSelect={() => last.set("chose Open")} />
          <MenuSeparator style={separator()} />
          <Submenu
            trigger={
              <View hoverStyle={{ backgroundColor: colors.border }} style={{ flexDirection: "row", alignItems: "center", height: 36, paddingHorizontal: 10, borderRadius: 8 }}>
                <Text style={{ flex: 1, fontSize: 14, color: colors.text }}>Share</Text>
                <Text style={{ fontSize: 14, color: colors.faint }}>›</Text>
              </View>
            }
            back={
              <View style={{ height: 36, paddingHorizontal: 10, justifyContent: "center" }}>
                <Text style={{ fontSize: 14, color: colors.accent }}>‹ Back</Text>
              </View>
            }
          >
            <Item label="Mail" onSelect={() => last.set("chose Mail")} />
            <Item label="Messages" onSelect={() => last.set("chose Messages")} />
            <Item label="Copy link" onSelect={() => last.set("chose Copy link")} />
          </Submenu>
          <MenuSeparator style={separator()} />
          <Item label="Delete" disabled />
        </Menu>
      </Card>
    );
  }

  function Context() {
    return (
      <Card icon="gestures" title="Context menu" subtitle={Platform.isDesktop ? "Right click the area" : "Hold a finger on the area"}>
        <ContextMenu
          margin={8}
          popupStyle={Platform.isDesktop ? popup() : sheet()}
          backdropStyle={backdrop}
          onOpenChange={(open, reason) => reason !== "select" && last.set(`context menu ${open ? "opened" : "closed"} (${reason})`)}
          trigger={
            <View style={{ height: 96, borderRadius: radius.inset, borderWidth: 1, borderStyle: "dashed", borderColor: colors.border, alignItems: "center", justifyContent: "center" }}>
              <Text style={{ fontSize: 14, color: colors.muted }}>{Platform.isDesktop ? "Right click here" : "Hold here"}</Text>
            </View>
          }
        >
          <Item label="Copy" onSelect={() => last.set("chose Copy")} />
          <Item label="Paste" onSelect={() => last.set("chose Paste")} />
        </ContextMenu>
      </Card>
    );
  }

  function Tooltips() {
    return (
      <Card icon="text" title="Tooltip" subtitle={Platform.isDesktop ? "Rest the pointer on the button" : "Hold a finger on the button"}>
        <Tooltip
          offset={6}
          delay={400}
          popupStyle={{ backgroundColor: colors.text, borderRadius: 6, paddingHorizontal: 8, paddingVertical: 4 }}
          content={<Text style={{ fontSize: 12, color: colors.background }}>Saves the document</Text>}
        >
          <Button label="Save" variant="secondary" onPress={() => last.set("pressed Save")} />
        </Tooltip>
      </Card>
    );
  }

  function Sheets() {
    const open = signal(false);
    return (
      <Card icon="components" title="Sheet" subtitle="A panel at the bottom, over a backdrop">
        <Button label="Show the sheet" variant="secondary" onPress={() => open.set(true)} />
        <Sheet
          open={open()}
          onOpenChange={(next, reason) => {
            open.set(next);
            last.set(`sheet ${next ? "opened" : "closed"} (${reason})`);
          }}
          popupStyle={sheet()}
          backdropStyle={backdrop}
        >
          <View style={{ padding: 12, gap: 12 }}>
            <Text style={{ fontSize: 17, fontWeight: "600", color: colors.text }}>Sheet</Text>
            <Text style={{ fontSize: 14, color: colors.muted }}>The backdrop, Escape and back close it.</Text>
            <Button label="Done" onPress={() => open.set(false)} />
          </View>
        </Sheet>
      </Card>
    );
  }

  function Last() {
    return (
      <Card icon="system" title="Last event">
        <Text style={{ fontSize: 13, color: colors.text, fontFamily: mono }}>{last()}</Text>
      </Card>
    );
  }

  return <DemoPage cards={[Primitive, Popovers, Menus, Context, Tooltips, Sheets, Last]} />;
}
