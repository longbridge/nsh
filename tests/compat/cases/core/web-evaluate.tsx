import { Text, View, WebView, signal, type WebViewRef } from "nsh";

export default function App() {
  const result = signal("Ready");
  let page!: WebViewRef;
  return <View style={{ flex: 1, paddingTop: 64 }}>
    <View testID="evaluate" onPress={async () => {
      let argument = "none";
      try { await page.evaluateJavaScript(123 as unknown as string); }
      catch (error) { argument = error instanceof TypeError ? "TypeError" : "wrong error"; }
      let code = "resolved";
      try { await page.evaluateJavaScript("1 + 2"); }
      catch (error) { code = (error as Error & { code: string }).code; }
      page.injectJavaScript("globalThis.compat = true");
      result.set(`Evaluation: ${argument}/${code}`);
    }}><Text>Evaluate</Text></View>
    <Text>{result()}</Text>
    <WebView ref={page} html="<title>Compat</title>" style={{ width: 200, height: 120 }} />
  </View>;
}
