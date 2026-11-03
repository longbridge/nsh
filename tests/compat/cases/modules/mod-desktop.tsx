// A case of the API's test suite (docs/dev/compatibility.md): `nsh/desktop`
// off the desktops (the headless runtime is iOS): `isAvailable` is false,
// `titleBar()` is all 0, `windowState()` is null and `displays()` empty,
// and the window (`Window.setBounds` too), app and menu functions are
// callable and do nothing, so an app that calls them runs on. Its script
// checks each.
import { Text, View, signal } from "nsh";
import { Application, Menu, Window, closeWindow, currentWindow, displays, focusWindow, isAvailable, openWindow, titleBar, windowState } from "nsh/desktop";

function App() {
  const calls = signal("not called");
  const popup = signal("not shown");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <Text>{`Desktop: ${isAvailable}`}</Text>
      <Text>{`Title bar: ${titleBar().height} ${titleBar().left} ${titleBar().right}`}</Text>
      <Text>{`Window state: ${windowState()} displays ${displays().length}`}</Text>
      <Text>{`Calls: ${calls()}`}</Text>
      <Text>{`Popup: ${popup()}`}</Text>
      <Text>{`Current window: ${currentWindow() === null ? "null" : "a window"}`}</Text>
      <View
        testID="window"
        onPress={() => {
          Window.setTitle("Compat");
          Window.setTitleBar({ style: "hidden", height: 40 });
          Window.setTitleBar({ style: "default" });
          Window.setFrame({ origin: { x: 40, y: 60 }, size: { width: 800, height: 600 }, animated: false });
          Window.setLevel("floating");
          Window.setLevel("normal");
          Window.orderFront();
          Window.setBounds({
            mode: "maximized",
            bounds: { origin: { x: -3000, y: 10 }, size: { width: 800, height: 600 } },
          });
          calls.set(`window done, close ${typeof Window.close}, minimize ${typeof Window.minimize}, setBounds ${typeof Window.setBounds}`);
        }}
      >
        <Text>Window calls</Text>
      </View>
      <View
        testID="menu"
        onPress={() => {
          Menu.setApplicationMenu([
            { label: "File", submenu: [{ label: "New", shortcut: "mod+n", icon: "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg'/%3E", onSelect: () => {} }, { type: "separator" }, { role: "quit" }] },
            { label: "Edit", submenu: [{ role: "copy" }, { role: "paste" }, false, { label: "Checked", checked: true, disabled: true }] },
          ]);
          Menu.setApplicationMenu(null);
          const shown = Menu.popup([{ label: "Rename", icon: { url: "asset:/rename.svg", size: { width: 16, height: 16 } }, onSelect: () => {} }], { x: 10, y: 20 });
          popup.set(typeof shown.then === "function" ? "a promise" : "no promise");
          shown.then(() => popup.set("closed"));
          calls.set("menu done");
        }}
      >
        <Text>Menu calls</Text>
      </View>
      <View
        testID="application"
        onPress={() => {
          Application.quit();
          Application.restart();
          calls.set(`application done, quit ${typeof Application.quit}, restart ${typeof Application.restart}`);
        }}
      >
        <Text>Application calls</Text>
      </View>
      <View
        testID="toggles"
        onPress={() => {
          Window.toggleMaximize();
          Window.toggleFullScreen();
          Window.toggleFullScreen();
          Window.toggleMaximize();
          calls.set("toggles done");
        }}
      >
        <Text>Toggle calls</Text>
      </View>
      <View
        testID="windows"
        onPress={() => {
          // A surface whose host has no windows of its own: opening one
          // throws, focusing and closing do nothing.
          let opened = "opened";
          try {
            openWindow({ name: "panel", params: { n: 1 }, title: "Panel", size: { width: 400, height: 300 }, position: { x: 10, y: 20 }, minSize: { width: 200, height: 100 }, resizable: false, focus: false, single: true });
          } catch (e) {
            opened = (e as Error).name;
          }
          focusWindow(1);
          closeWindow(1);
          calls.set(`windows: open ${opened}`);
        }}
      >
        <Text>Window functions</Text>
      </View>
    </View>
  );
}

export default App;
