// A case of the API's test suite (docs/dev/compatibility.md): the keys of a
// focus a dialog covers, run as on macOS (`platform macos`). From the
// keyboard, a menu's item opens a dialog whose Return confirms; the menu
// closes and gives the focus back to its trigger, under the dialog. Return
// is then the dialog's: the trigger's own Return (its `forFocus` binding,
// which opens its menu) and its press wait until the dialog closes.
import { keymap, Text, View, signal } from "nsh";
import { Dialog, Menu, MenuItem } from "nsh/base";

// The dialog's content: its Return, bound while it shows.
function Confirm(props: { onConfirm: () => void }) {
  keymap.bind("enter", props.onConfirm, { when: "notEditing" });
  return <Text>Really delete?</Text>;
}

function App() {
  const confirming = signal(false);
  const confirms = signal(0);
  return (
    <View style={{ flex: 1, paddingTop: 64, paddingLeft: 20 }}>
      <Text>{`Confirms: ${confirms()}`}</Text>
      <Menu trigger={<View style={{ width: 80, height: 30 }}><Text>More</Text></View>}>
        <MenuItem onSelect={() => confirming.set(true)}>
          <Text>Delete</Text>
        </MenuItem>
        <MenuItem>
          <Text>Rename</Text>
        </MenuItem>
      </Menu>
      <Dialog open={confirming()} onOpenChange={({ open }) => confirming.set(open)}>
        <If when={confirming()}>
          <Confirm
            onConfirm={() => {
              confirms.set(confirms() + 1);
              confirming.set(false);
            }}
          />
        </If>
      </Dialog>
    </View>
  );
}

export default App;
