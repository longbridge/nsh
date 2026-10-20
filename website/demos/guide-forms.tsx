import { Input, mount, Scroll, signal, Text, View, type YNode } from "nsh";
import { ui } from "./palette";

const field = () => ({ ...ui().inset, fontSize: 16, color: ui().text, placeholderColor: ui().faint, borderRadius: 12, paddingHorizontal: 12, paddingVertical: 10 });
const label = () => ({ fontSize: 13, color: ui().muted });

function App() {
  const name = signal("");
  const email = signal("");
  const code = signal("");
  const notes = signal("");
  const notesHeight = signal(0);
  const focused = signal("nothing");
  const sent = signal("");
  let emailInput!: YNode;
  let passwordInput!: YNode;
  let notesInput!: YNode;

  let tries = 0;
  const submit = () => sent.set(`#${++tries}: signed up ${name() || "nobody"} <${email() || "no e-mail"}>`);

  return (
    <Scroll style={{ flex: 1, backgroundColor: ui().page }} keyboardDismissMode="on-drag" keyboardShouldPersistTaps="handled">
      <View style={{ padding: 16, paddingTop: 24, gap: 12 }}>
        <Text style={{ fontSize: 22, fontWeight: "700", color: ui().text }}>Sign up</Text>
        <Text style={label()}>Focused: {focused()}</Text>

        <Text style={label()}>Name (controlled, return goes to the next field)</Text>
        <Input
          style={field()}
          value={name()}
          onChange={(t) => name.set(t)}
          placeholder="Ada Lovelace"
          autoCapitalize="words"
          autoComplete="name"
          returnKeyType="next"
          submitBehavior="submit"
          onSubmit={() => emailInput.focus()}
          onFocus={() => focused.set("name")}
        />

        <Text style={label()}>E-mail</Text>
        <Input
          ref={emailInput}
          style={field()}
          value={email()}
          onChange={(t) => email.set(t.trim().toLowerCase())}
          placeholder="ada@example.com"
          keyboardType="email-address"
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="email"
          returnKeyType="next"
          submitBehavior="submit"
          onSubmit={() => passwordInput.focus()}
          onFocus={() => focused.set("e-mail")}
        />

        <Text style={label()}>Password (uncontrolled)</Text>
        <Input
          ref={passwordInput}
          style={field()}
          masked
          autoComplete="new-password"
          placeholder="At least 8 characters"
          returnKeyType="next"
          submitBehavior="submit"
          onSubmit={() => notesInput.focus()}
          onFocus={() => focused.set("password")}
        />

        <Text style={label()}>Code (number pad, 6 digits)</Text>
        <Input
          style={{ ...field(), letterSpacing: 8, textAlign: "center", fontSize: 22 }}
          value={code()}
          onChange={(t) => code.set(t.replace(/[^0-9]/g, ""))}
          keyboardType="number-pad"
          autoComplete="one-time-code"
          maxLength={6}
          onFocus={() => focused.set("code")}
        />

        <Text style={label()}>About you (grows with its text, 200 max)</Text>
        <Input
          ref={notesInput}
          multiline
          maxLength={200}
          value={notes()}
          onChange={(t) => notes.set(t)}
          onContentSizeChange={(e) => notesHeight.set(Math.min(e.height, 120))}
          placeholder="Return adds a line"
          style={{ ...field(), height: Math.max(notesHeight(), 22) + 20 }}
          onFocus={() => focused.set("notes")}
          onBlur={() => focused.set("nothing")}
        />
        <Text style={{ ...label(), textAlign: "right" }}>{`${notes().length}/200`}</Text>

        <View
          onPress={submit}
          activeStyle={{ opacity: 0.6 }}
          style={{
            padding: 14,
            borderRadius: 10,
            ...ui().primary,
            alignItems: "center",
          }}
        >
          <Text style={{ color: ui().onPrimary, fontWeight: "600" }}>Sign up</Text>
        </View>
        <Text style={{ color: ui().up }}>{sent()}</Text>
      </View>
    </Scroll>
  );
}

mount(App);
