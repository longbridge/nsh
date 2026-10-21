import { cleanup, mount, signal, Scroll, Text, View } from "nsh";
import { TextView } from "nsh/base";
import { ui } from "./palette";

const REPLY = `## Moving to offline sync

Sync now runs **in the background**, and conflicts merge *per field*:

1. Export with \`nsh export\`
2. Check the dry run
3. Switch over

| Step   | Time  |
|--------|------:|
| Export | 2 min |
| Check  | 5 min |

\`\`\`sh
nsh export --out backup.json
\`\`\`

> [!TIP]
> Long-press a paragraph to copy it.`;

function App() {
  const text = signal(REPLY);
  const streaming = signal(false);
  let timer: number | undefined;
  const stop = () => {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
    streaming.set(false);
  };
  cleanup(stop);
  // Replays the reply a few characters at a time, as a model streams it.
  const play = () => {
    stop();
    const tokens = REPLY.match(/\s*\S{1,4}|\s+/g) ?? [];
    let i = 0;
    text.set("");
    streaming.set(true);
    timer = setInterval(() => (i < tokens.length ? text.set(text() + tokens[i++]) : stop()), 40);
  };
  return (
    <Scroll style={{ flex: 1, backgroundColor: ui().page }}>
      <View style={{ padding: 20, gap: 16 }}>
        <TextView
          markdown={text()}
          options={{ streaming: streaming() }}
          caret={streaming() ? <View style={{ width: 8, height: 16, marginLeft: 2, borderRadius: 2, backgroundColor: ui().accent }} /> : undefined}
          selectable
        />
        <View
          onPress={play}
          activeStyle={{ opacity: 0.75 }}
          style={{ ...ui().raised, borderRadius: 12, paddingVertical: 10, alignItems: "center", opacity: streaming() ? 0.5 : 1 }}
        >
          <Text style={{ color: ui().text, fontSize: 15, fontWeight: "600" }}>{streaming() ? "Streaming…" : "Stream it again"}</Text>
        </View>
      </View>
    </Scroll>
  );
}

mount(() => <App />);
