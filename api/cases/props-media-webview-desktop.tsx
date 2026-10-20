// A case of the API's test suite (docs/dev/compatibility.md): a web view's
// desktop props, run as on macOS (`platform macos`): the swipes back and
// forward from `desktop={{…}}`, not the ones `ios={{…}}` turns off. Kept
// with `cargo xtask api --keep api/cases/props-media-webview-desktop.tsx`;
// its script checks it.
import { View, mount } from "nsh";
import { WebView } from "nsh/webview";

function App() {
  return (
    <View style={{ flex: 1 }}>
      <WebView
        testID="page"
        source="https://example.com/"
        ios={{ allowsBackForwardNavigationGestures: false }}
        desktop={{ allowsBackForwardNavigationGestures: true }}
        style={{ width: 400, height: 300 }}
      />
    </View>
  );
}

mount(App);
