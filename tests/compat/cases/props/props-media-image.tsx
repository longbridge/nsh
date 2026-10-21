// A case of the API's test suite (docs/dev/compatibility.md): what an image
// shows, as the platform receives it: its source (a URL, then another), how
// it fits its box, the placeholder until it loads, a blur and how long it
// fades in. Its script checks each.
import { Text, View, signal } from "nsh";
import { Image } from "nsh/image";

function App() {
  const photo = signal("https://example.com/a.png");
  return (
    <View style={{ flex: 1, paddingTop: 64 }}>
      <View testID="next" onPress={() => photo.set("https://example.com/b.png")}>
        <Text>Next</Text>
      </View>
      <Image
        testID="photo"
        src={photo()}
        resizeMode="contain"
        placeholder="https://example.com/placeholder.png"
        blurRadius={6}
        fadeDuration={150}
        style={{ width: 120, height: 80 }}
      />
      <Image testID="plain" src="https://example.com/plain.png" style={{ width: 40, height: 40 }} />
    </View>
  );
}

export default App;
