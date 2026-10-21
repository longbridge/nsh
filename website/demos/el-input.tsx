import { Input, mount, signal, Text, View, type YNode } from "nsh";
import { ui } from "./palette";

function App() {
  const name = signal("");
  const password = signal("");
  const notes = signal("");
  const submitted = signal("");
  let passwordInput!: YNode;
  const field = () => ({ ...ui().inset, fontSize: 16, color: ui().text, placeholderColor: ui().faint, borderRadius: 12, paddingHorizontal: 12, paddingVertical: 10 });
  return (
    <View style={{ flex: 1, backgroundColor: ui().page, padding: 20, paddingTop: 24, gap: 10 }}>
      <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>input</Text>

      <Text style={{ color: ui().muted }}>Name (controlled, maxLength 20)</Text>
      <Input
        style={field()}
        value={name()}
        onChange={(text) => name.set(text)}
        placeholder="Ada Lovelace"
        autoCapitalize="words"
        maxLength={20}
        returnKeyType="next"
        onSubmit={() => passwordInput.focus()}
      />
      <Text style={{ fontSize: 12, color: ui().muted }}>{name().length} / 20</Text>

      <Text style={{ color: ui().muted }}>Password</Text>
      <Input
        ref={passwordInput}
        style={field()}
        value={password()}
        onChange={(text) => password.set(text)}
        masked
        autoComplete="current-password"
        returnKeyType="done"
        onSubmit={() => submitted.set(`Signed in as ${name() || "nobody"}`)}
      />

      <Text style={{ color: ui().muted }}>Notes (multiline)</Text>
      <Input
        style={{ ...field(), height: 100 }}
        multiline
        value={notes()}
        onChange={(text) => notes.set(text)}
        placeholder="Anything else?"
      />

      <View
        onPress={() => submitted.set(`Signed in as ${name() || "nobody"}`)}
        activeStyle={{ opacity: 0.7 }}
        disabled={password().length === 0}
        class="disabled:opacity-40"
        style={{
          ...ui().primary,
          borderRadius: 10,
          padding: 14,
          alignItems: "center",
          marginTop: 6,
        }}
      >
        <Text style={{ color: ui().onPrimary, fontSize: 16, fontWeight: "600" }}>Sign in</Text>
      </View>
      <Text style={{ color: ui().up }}>{submitted()}</Text>
    </View>
  );
}

mount(App);
