import { mount, signal, Text, View } from "nsh";
import { ui } from "./palette";

interface Todo {
  id: number;
  title: string;
  done: boolean;
}

let nextId = 4;

function App() {
  const todos = signal<Todo[]>([
    { id: 1, title: "Read the elements page", done: true },
    { id: 2, title: "Try <If> and <Each>", done: false },
    { id: 3, title: "Ship the app", done: false },
  ]);
  const showDone = signal(true);
  const visible = () => (showDone() ? todos() : todos().filter((t) => !t.done));
  const toggle = (id: number) => todos.update((list) => list.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));
  const add = () => todos.update((list) => [...list, { id: nextId++, title: `Task ${nextId - 1}`, done: false }]);
  const remove = (id: number) => todos.update((list) => list.filter((t) => t.id !== id));
  const button = { borderRadius: 10, paddingHorizontal: 12, paddingVertical: 8 } as const;

  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 12 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>if / each</Text>
      <View style={{ flexDirection: "row", gap: 8 }}>
        <View
          onPress={add}
          activeStyle={{ opacity: 0.6 }}
          style={{ ...ui().accentFill, ...button }}
        >
          <Text style={{ color: ui().onAccent, fontWeight: "600" }}>Add</Text>
        </View>
        <View
          onPress={() => showDone.set(!showDone())}
          activeStyle={{ opacity: 0.6 }}
          style={{ ...ui().raised, ...button }}
        >
          <Text style={{ color: ui().text }}>{showDone() ? "Hide done" : "Show done"}</Text>
        </View>
      </View>

      <If
        when={visible().length > 0}
        else={<Text style={{ color: ui().muted, textAlign: "center", marginTop: 40 }}>Nothing to show.</Text>}
      >
        <View style={{ gap: 8 }}>
          <Each of={visible()} key={(t: Todo) => t.id}>
            {(todo, index) => (
              <View
                style={{ ...ui().row, flexDirection: "row", alignItems: "center", gap: 12, borderRadius: 12, padding: 12 }}
              >
                <View
                  onPress={() => toggle(todo().id)}
                  hitSlop={8}
                  style={{
                    width: 22,
                    height: 22,
                    borderRadius: 11,
                    borderWidth: 2,
                    borderColor: ui().accent,
                    backgroundColor: todo().done ? ui().accent : "transparent",
                  }}
                />
                <Text
                  style={{
                    flex: 1,
                    color: todo().done ? ui().faint : ui().text,
                    textDecorationLine: todo().done ? "line-through" : "none",
                  }}
                >
                  {index() + 1}. {todo().title}
                </Text>
                <If when={todo().done}>
                  <View onPress={() => remove(todo().id)} hitSlop={8}>
                    <Text style={{ color: ui().down }}>Remove</Text>
                  </View>
                </If>
              </View>
            )}
          </Each>
        </View>
      </If>
      <Text style={{ fontSize: 12, color: ui().muted, fontVariant: ["tabular-nums"] }}>
        {todos().filter((t) => t.done).length} of {todos().length} done
      </Text>
    </View>
  );
}

mount(App);
