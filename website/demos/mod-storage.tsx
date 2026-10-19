// Storage: a launch counter kept across reloads, the stored keys and a reset.
import { mount, signal, Text, View } from "nsh";
import { Storage } from "nsh/storage";
import { Button } from "nsh/ui/mobile";
import { ui } from "./palette";

function App() {
  const opened = signal<number | null>(null);
  const keys = signal<string[]>([]);

  const start = async () => {
    const count = Number((await Storage.getItem("opened")) ?? "0") + 1;
    await Storage.setItem("opened", String(count));
    await Storage.setItem("lastOpened", new Date().toISOString());
    opened.set(count);
    keys.set(await Storage.getAllKeys());
  };
  const reset = async () => {
    await Storage.clear();
    opened.set(0);
    keys.set(await Storage.getAllKeys());
  };
  start();

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 16, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>Storage</Text>
      <View style={{ borderRadius: 14, padding: 16, gap: 6, ...ui().card }}>
        <Text style={{ fontSize: 28, fontWeight: "700", color: ui().text }}>
          {opened() === null ? "Reading…" : `Opened ${opened()} times`}
        </Text>
        <Text style={{ color: ui().muted }}>Reload the page: the count is still there.</Text>
      </View>
      <Text style={{ color: ui().muted }}>{`Keys: ${keys().length ? keys().join(", ") : "none"}`}</Text>
      <Button title="Reset" variant="secondary" onPress={reset} />
    </View>
  );
}

mount(App);
