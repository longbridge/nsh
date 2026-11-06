import { cleanup, signal } from "nsh";
import { GlobalShortcut, isAvailable, Tray, Window } from "nsh/desktop";
import star from "../star.svg";
import { Badge, Button, Caption, Card, DemoPage, Row } from "../ui";

/**
 * `Tray` and `GlobalShortcut` of `nsh/desktop`: the app's icon in the system
 * tray with a tooltip, a click and a menu, and a keystroke heard whichever
 * app has the keyboard.
 */
export function TrayDemo() {
  const shown = signal(false);
  const last = signal("Nothing yet");
  const muted = signal(false);
  const registered = signal("Not registered");
  const presses = signal(0);
  const showTray = () => {
    shown.set(true);
    Tray.set({
      icon: star,
      tooltip: "NSH Gallery",
      onClick: () => last.set("Clicked the icon"),
      menu: [
        { label: "Open Gallery", onSelect: () => (Window.orderFront(), last.set("Chose Open Gallery")) },
        { label: "Mute", checked: muted(), onSelect: () => (muted.set(!muted()), showTray(), last.set(`Muted: ${muted()}`)) },
        { type: "separator" },
        { label: "More", submenu: [{ label: "About", onSelect: () => last.set("Chose About") }] },
        { label: "Remove icon", onSelect: () => hideTray() },
      ],
    });
  };
  const hideTray = () => {
    shown.set(false);
    Tray.set(null);
  };
  const shortcut = "ctrl+alt+k";
  const register = () =>
    void GlobalShortcut.register(shortcut, () => presses.set(presses() + 1)).then((ok) =>
      registered.set(ok ? "Registered" : "Refused"),
    );
  const unregister = () => {
    GlobalShortcut.unregister(shortcut);
    registered.set("Not registered");
  };
  cleanup(() => {
    Tray.set(null);
    GlobalShortcut.unregisterAll();
  });
  return (
    <DemoPage
      cards={[
        () => (
          <Card title="Tray icon" subtitle={isAvailable ? "Click it, or open its menu" : "Desktops only"}>
            <Row first>
              <Button label={shown() ? "Update" : "Show icon"} onPress={showTray} />
              <Button label="Remove" variant="ghost" onPress={hideTray} />
            </Row>
            <Caption label={last()} />
          </Card>
        ),
        () => (
          <Card title="Global shortcut" subtitle="Ctrl+Alt+K, from any app">
            <Row first>
              <Button label="Register" onPress={register} />
              <Button label="Unregister" variant="ghost" onPress={unregister} />
              <Badge label={registered()} />
            </Row>
            <Caption label={`Pressed ${presses()} times`} />
          </Card>
        ),
      ]}
    />
  );
}
