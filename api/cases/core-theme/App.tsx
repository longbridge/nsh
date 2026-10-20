// A case of the API's test suite (docs/dev/compatibility.md): what the
// bundler emits for an app's own theme and fonts. nsh.json's Tailwind
// theme reaches the renderer through `nsh:rt`'s `setup` (a spacing step of
// 8 points makes `w-10` 80 points wide), and a font import is `font`, which
// evaluates to the font's family. Kept with
// `cargo xtask api --keep api/cases/core-theme`; its script checks each.
import { Text, View, mount } from "nsh";
import Pacifico from "./fonts/Pacifico-Regular.ttf";

function App() {
  return (
    <View testID="root" style={{ flex: 1, paddingTop: 64 }}>
      <Text style={{ fontFamily: Pacifico }}>Family: {Pacifico}</Text>
      <View testID="spaced" class="w-10 h-5" />
    </View>
  );
}

mount(App);
