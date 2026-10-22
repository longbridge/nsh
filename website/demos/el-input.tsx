import { Input, type JSX, mount, signal, Text, View, type YNode } from "nsh";
import { Inspector, Panel, size, Stage, ui, wide } from "./palette";

function App() {
  const name = signal("");
  const password = signal("");
  const notes = signal("");
  const submitted = signal("");
  let passwordInput!: YNode;
  const signIn = () => submitted.set(`Signed in as ${name() || "nobody"}`);
  // Touch fields on a phone; compact ones, 32 points high, in a desktop layout.
  const field = () =>
    wide()
      ? { ...ui().inset, fontSize: 14, color: ui().text, placeholderColor: ui().faint, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 7 }
      : { ...ui().inset, fontSize: 16, color: ui().text, placeholderColor: ui().faint, borderRadius: 12, paddingHorizontal: 12, paddingVertical: 10 };

  const nameField = () => (
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
  );
  const count = () => <Text style={{ fontSize: 12, color: ui().muted }}>{name().length} / 20</Text>;
  const passwordField = () => (
    <Input
      ref={passwordInput}
      style={field()}
      value={password()}
      onChange={(text) => password.set(text)}
      masked
      autoComplete="current-password"
      returnKeyType="done"
      onSubmit={signIn}
    />
  );
  const notesField = () => (
    <Input
      style={{ ...field(), height: wide() ? 80 : 100 }}
      multiline
      value={notes()}
      onChange={(text) => notes.set(text)}
      placeholder="Anything else?"
    />
  );
  const button = () => (
    <View
      onPress={signIn}
      activeStyle={{ opacity: 0.7 }}
      disabled={password().length === 0}
      class="disabled:opacity-40"
      style={
        wide()
          ? { ...ui().primary, alignSelf: "flex-start", borderRadius: 8, height: size.control, paddingHorizontal: 14, justifyContent: "center", marginTop: 4 }
          : { ...ui().primary, borderRadius: 10, padding: 14, alignItems: "center", marginTop: 6 }
      }
    >
      <Text style={{ color: ui().onPrimary, fontSize: wide() ? 13 : 16, fontWeight: "600" }}>Sign in</Text>
    </View>
  );
  /** A label above its field, and what describes it below. */
  const group = (label: string, input: JSX.Element, after?: JSX.Element) => (
    <View style={{ gap: 6 }}>
      <Text style={{ fontSize: 13, color: ui().muted }}>{label}</Text>
      {input}
      {after}
    </View>
  );

  return (
    <View style={{ flex: 1, backgroundColor: ui().page }}>
      {() =>
        wide() ? (
          <View style={{ flex: 1, padding: 20, paddingTop: 24, gap: 14 }}>
            <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>input</Text>
            <Text style={{ color: ui().muted }}>Tab moves between the fields; Return in the name goes to the password</Text>
            <View style={{ flex: 1, flexDirection: "row", gap: 16 }}>
              <Stage width={340} gap={16}>
                {group("Name (controlled, maxLength 20)", nameField(), count())}
                {group("Password", passwordField())}
                {group("Notes (multiline)", notesField())}
                {button()}
              </Stage>
              <Inspector>
                <Panel title="value">
                  <Text style={{ fontSize: 13, color: ui().text }}>name {name() ? `“${name()}”` : "—"}</Text>
                  <Text style={{ fontSize: 13, color: ui().text, fontVariant: ["tabular-nums"] }}>password {password().length} characters</Text>
                  <Text style={{ fontSize: 13, color: ui().text, fontVariant: ["tabular-nums"] }}>notes {notes().length} characters</Text>
                </Panel>
                <Panel title="onSubmit" grow>
                  <Text style={{ fontSize: 13, color: ui().up }}>{submitted()}</Text>
                </Panel>
              </Inspector>
            </View>
          </View>
        ) : (
          <View style={{ flex: 1, width: "100%", maxWidth: 480, alignSelf: "center", padding: 20, paddingTop: 24, gap: 10 }}>
            <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>input</Text>

            <Text style={{ color: ui().muted }}>Name (controlled, maxLength 20)</Text>
            {nameField()}
            {count()}

            <Text style={{ color: ui().muted }}>Password</Text>
            {passwordField()}

            <Text style={{ color: ui().muted }}>Notes (multiline)</Text>
            {notesField()}

            {button()}
            <Text style={{ color: ui().up }}>{submitted()}</Text>
          </View>
        )
      }
    </View>
  );
}

mount(App);
