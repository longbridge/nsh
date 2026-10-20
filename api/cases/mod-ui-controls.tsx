// A case of the API's test suite (docs/dev/compatibility.md): the controls
// of `nsh/ui/mobile`: a `Button` pressed (and one disabled), `Stepper`s
// driven through their refs and `onChange`, a `SelectList` choosing an
// option, and a `TextField` cleared by its clear button. Kept with
// `cargo xtask api --keep api/cases/mod-ui-controls.tsx`; its script checks
// each.
import { Text, View, mount, signal } from "nsh";
import { Button, SelectList, Stepper, type StepperRef, TextField } from "nsh/ui/mobile";

function App() {
  const presses = signal(0);
  const disabled = signal(0);
  const quantity = signal(2);
  const asked = signal("not asked");
  const choice = signal("none");
  const text = signal("not changed");
  let own!: StepperRef;
  let controlled!: StepperRef;
  return (
    <View style={{ flex: 1, paddingTop: 64, gap: 4 }}>
      <Button testID="press" title="Press me" onPress={() => presses.set(presses() + 1)} />
      <Text>{`Presses: ${presses()}`}</Text>
      <Button title="Disabled" variant="danger" disabled onPress={() => disabled.set(disabled() + 1)} />
      <Button title="Plain" variant="plain" accessibilityLabel="Plain button" onPress={() => presses.set(presses() + 10)} />
      <Text>{`Disabled presses: ${disabled()}`}</Text>
      <Stepper ref={own} defaultValue={5} min={0} max={8} step={1} />
      <Stepper ref={controlled} value={quantity()} min={0} max={10} step={0.5} onChange={(v) => quantity.set(v)} />
      <Text>{`Quantity: ${quantity()}`}</Text>
      <Text>{`Asked: ${asked()}`}</Text>
      <Button
        title="Ask the stepper"
        variant="secondary"
        onPress={async () => {
          const before = await own.value();
          own.setValue(20);
          asked.set(`${before} then ${await own.value()}`);
        }}
      />
      <Button title="Set the quantity" variant="secondary" onPress={() => controlled.setValue(3.5)} />
      <View style={{ height: 150 }}>
        <SelectList options={["Alpha", "Beta", "Gamma"]} onSelect={(i) => choice.set(String(i))} />
      </View>
      <Text>{`Chosen: ${choice()}`}</Text>
      <TextField
        label="Name"
        placeholder="Your name"
        defaultValue="Ada"
        onChange={(t) => text.set(t === "" ? "emptied" : t)}
      />
      <Text>{`Text: ${text()}`}</Text>
    </View>
  );
}

mount(App);
