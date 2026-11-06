// A case of the API's test suite (docs/dev/compatibility.md): what a view
// or text is in a document (`semantic`, the HTML element the web draws it
// as) and where a link view leads (`href`). The platform receives both
// with the view's props; a press on the link stays the view's `onPress`.
// Its script checks each.
import { Text, View, signal } from "nsh";

function App() {
  const presses = signal(0);
  return (
    <View testID="article" semantic="article" style={{ flex: 1, paddingTop: 64 }}>
      <Text testID="heading" semantic="h1">
        Notes
      </Text>
      <Text testID="paragraph" semantic="p">
        A paragraph
      </Text>
      <View testID="link" semantic="a" href="/notes/q3" onPress={() => presses.set(presses() + 1)}>
        <Text>Open note</Text>
      </View>
      <Text>{`Presses: ${presses()}`}</Text>
    </View>
  );
}

export default App;
