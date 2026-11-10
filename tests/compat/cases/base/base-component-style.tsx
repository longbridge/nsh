// A case of the API's test suite (docs/dev/compatibility.md): the looks
// an app declares once for `nsh/base`'s components (its theme's
// `"components"`, beside `cssVars`), picked by name: a `Button`'s `variant`
// and `size` take the classes the theme gives them, its tokens resolved; a
// button that picks none takes no look. Its script checks each.
import { Text, View } from "nsh";
import { Button } from "nsh/base";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 20, gap: 4 }}>
      <Button testID="buy" variant="primary" size="lg">
        <Text>Buy</Text>
      </Button>
      <Button testID="plain">
        <Text>Plain</Text>
      </Button>
    </View>
  );
}

export default App;
