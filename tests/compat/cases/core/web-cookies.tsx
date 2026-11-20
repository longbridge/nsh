import { Text, View, WebView, signal, type WebViewRef } from "nsh";

export default function App() {
  const result = signal("Ready");
  let page!: WebViewRef;
  return <View style={{ flex: 1, paddingTop: 64 }}>
    <View testID="cookies" onPress={async () => {
      const available = await WebView.cookiesAvailable();
      const failures: string[] = [];
      for (const call of [
        () => page.getCookies(123 as unknown as string),
        () => page.getCookies("https://example.com", { timeout: -1 }),
      ]) {
        try { await call(); failures.push("resolved"); }
        catch (error) { failures.push((error as Error).name); }
      }
      let code = "resolved";
      try { await page.getCookies("https://example.com", { timeout: 1000 }); }
      catch (error) { code = (error as Error & { code: string }).code; }
      result.set(`Cookies: ${available}/${failures.join(",")}/${code}`);
    }}><Text>Cookies</Text></View>
    <Text>{result()}</Text>
    <WebView ref={page} html="<title>Compat</title>" style={{ width: 200, height: 120 }} />
  </View>;
}
