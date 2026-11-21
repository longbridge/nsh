import { mount, platform, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";
import { Button, Dialog } from "nsh/base";

type Shown = "none" | "first" | "wrapped" | "earlier" | "later";

// Page sheet dialogs, one replacing another in a single press: "earlier"
// is written before "first", "later" after it, and "wrapped" is closed by
// removing the view it is in, not the dialog itself.
function App() {
  const shown = signal<Shown>("none");
  const count = signal(0);
  const closed = signal<string[]>([]);
  const sheet = (name: Shown, title: string, next: { label: string; to: Shown }[]) => (
    <Dialog
      open={shown() === name}
      presentation="pageSheet"
      onOpenChange={({ open }) => {
        if (!open && shown() === name) shown.set("none");
      }}
      onClose={() => closed.update((all) => [...all, name])}
      style={{ flex: 1 }}
    >
      <View style={{ flex: 1, padding: 24, gap: 12 }}>
        <Text>{title}</Text>
        {next.map(({ label, to }) => (
          <Button onPress={() => shown.set(to)}>
            <Text>{label}</Text>
          </Button>
        ))}
        <Button onPress={() => count.update((n) => n + 1)}>
          <Text>{`Count in ${title}`}</Text>
        </Button>
      </View>
    </Dialog>
  );
  return (
    <View style={{ flex: 1, paddingTop: 80, paddingHorizontal: 24, gap: 12 }}>
      <Text>{`shown: ${shown()}`}</Text>
      <Text>{`count: ${count()}`}</Text>
      <Text>{`closed: ${closed().join(", ")}`}</Text>
      <Text>{`Platform: ${platform.os}`}</Text>
      <Button onPress={() => shown.set("first")}>
        <Text>Open first</Text>
      </Button>
      <Button onPress={() => shown.set("wrapped")}>
        <Text>Open wrapped</Text>
      </Button>
      {sheet("earlier", "Earlier sheet", [])}
      {sheet("first", "First sheet", [
        { label: "Replace with earlier", to: "earlier" },
        { label: "Replace with later", to: "later" },
      ])}
      <If when={shown() === "wrapped"}>
        <View>
          {/* Open while the view is there: closed with it. */}
          <Dialog open presentation="pageSheet" onClose={() => closed.update((all) => [...all, "wrapped"])} style={{ flex: 1 }}>
            <View style={{ flex: 1, padding: 24, gap: 12 }}>
              <Text>Wrapped sheet</Text>
              <Button onPress={() => shown.set("later")}>
                <Text>Replace wrapped with later</Text>
              </Button>
            </View>
          </Dialog>
        </View>
      </If>
      {sheet("later", "Later sheet", [])}
    </View>
  );
}

mount(() => <Window id="main">{App}</Window>);
