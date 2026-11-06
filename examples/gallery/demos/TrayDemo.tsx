import { cleanup, signal } from "nsh";
import { isAvailable, Tray, Window } from "nsh/desktop";
import star from "../star.svg";
import { Button, Caption, Card, DemoPage, Row } from "../ui";

/**
 * `Tray` of `nsh/desktop`: the app's icon in the system tray with a
 * tooltip, a click and a menu.
 */
export function TrayDemo() {
  const shown = signal(false);
  const last = signal("Nothing yet");
  const muted = signal(false);
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
  cleanup(() => Tray.set(null));
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
      ]}
    />
  );
}
