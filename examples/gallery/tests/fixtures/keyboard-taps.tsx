import { keyboardFrame, mount, Scroll, signal, Text, View } from "nsh";
import { ContextMenu, Input, MenuItem } from "nsh/base";

function App() {
  const presses = signal(0);
  const held = signal(0);
  const drags = signal(0);
  return (
    <View style={{ flex: 1, paddingTop: 80, paddingHorizontal: 24 }}>
      <Text>{keyboardFrame().height > 0 ? "keyboard: up" : "keyboard: down"}</Text>
      <Text>{`drags: ${drags()}`}</Text>
      {/* Short enough that the keyboard leaves the field where it is: the rows stay on screen. */}
      <Scroll style={{ height: 360 }} onScrollDragStart={() => drags.set(drags() + 1)}>
        <View style={{ height: 60 }} />
        <View role="button" onPress={() => presses.set(presses() + 1)} style={{ height: 60, justifyContent: "center", padding: 16 }}>
          <Text>{`Row 1, pressed ${presses()}`}</Text>
        </View>
        <ContextMenu
          trigger={
            <View role="button" onPress={() => presses.set(presses() + 1)} style={{ height: 60, justifyContent: "center", padding: 16 }}>
              <Text>Row 2</Text>
            </View>
          }
        >
          <MenuItem>
            <Text>menu-row</Text>
          </MenuItem>
        </ContextMenu>
        <View role="button" onPress={() => presses.set(presses() + 1)} onLongPress={() => held.set(held() + 1)} style={{ height: 60, justifyContent: "center", padding: 16 }}>
          <Text>{`Row 3, held ${held()}`}</Text>
        </View>
        <View style={{ height: 1600 }} />
      </Scroll>
      <Input testID="message" placeholder="Message" style={{ height: 40, margin: 12 }} />
    </View>
  );
}

mount(App);
