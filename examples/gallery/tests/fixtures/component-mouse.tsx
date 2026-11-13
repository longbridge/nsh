import { mount, signal, Text, View } from "nsh";
import type { MouseEvent } from "nsh";
import { Button, Link, Toggle } from "nsh/base";
import { Line, Plot } from "nsh/base/plot";

function App() {
  const heard = signal<string[]>([]);
  const say = (s: string) => heard.set([...heard(), s]);
  const at = (e: MouseEvent) => `${e.position.x},${e.position.y}`;
  const pinned = signal(false);
  return (
    <View class="flex-1 bg-background pt-16 px-4 gap-3">
      <Button
        testID="preview"
        class="h-10 items-center justify-center rounded-md bg-primary"
        onMouseEnter={(e) => say(`button enter ${at(e)}`)}
        onMouseMove={(e) => say(`button move ${at(e)}`)}
        onMouseLeave={() => say("button leave")}
      >
        <Text class="text-primary-foreground">Preview</Text>
      </Button>
      <Toggle
        testID="pin"
        pressed={pinned()}
        onChange={({ pressed }) => pinned.set(pressed)}
        class="h-10 items-center justify-center rounded-md border border-border"
        onMouseEnter={() => say("toggle enter")}
        onMouseLeave={() => say("toggle leave")}
      >
        <Text class="text-foreground">Pin</Text>
      </Toggle>
      <Link testID="docs" href="/docs" onMouseEnter={() => say("link enter")}>
        <Text class="text-foreground underline">Docs</Text>
      </Link>
      {/* A plot follows the mouse itself (its hover); the app hears it too. */}
      <Plot
        testID="chart"
        data={[{ price: 1 }, { price: 3 }, { price: 2 }]}
        y="price"
        onHoverChange={({ index }) => say(`plot hover ${index}`)}
        onMouseMove={() => say("chart move")}
        style={{ height: 60 }}
      >
        <Line />
      </Plot>
      <Button testID="quiet" class="h-10 items-center justify-center rounded-md bg-secondary">
        <Text class="text-secondary-foreground">Quiet</Text>
      </Button>
      <Text class="text-muted-foreground">{`heard: ${heard().join("; ") || "nothing"}`}</Text>
    </View>
  );
}

mount(App);
