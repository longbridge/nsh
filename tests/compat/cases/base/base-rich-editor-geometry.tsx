// Intrinsic extent and internal scroll offset are the geometry event's
// complete payload, independent of the viewport's minimum height.
import { Text, View, signal } from "nsh";
import { RichEditor, createRichEditor } from "nsh/base";

export default function App() {
  const editor = createRichEditor({ content: "First line\n\nLast line", format: "markdown" });
  const narrow = signal(false);
  const extent = signal("Extent: waiting");
  const width = signal("Width: waiting");
  return (
    <View style={{ flex: 1, paddingTop: 60 }}>
      <View testID="geometry-narrow" onPress={() => narrow.set(true)}><Text>Narrow editor</Text></View>
      <RichEditor editor={editor} style={{ width: narrow() ? 120 : 240, height: 100, flexShrink: 0 }} onGeometryChange={({ contentSize, scrollOffset }) => {
        extent.set(`Extent: ${contentSize.width > 0} ${contentSize.height > 0 && contentSize.height < 100} ${scrollOffset === 0}`);
        width.set(`Width: ${contentSize.width <= 120}`);
      }} />
      <Text>{extent()}</Text>
      <Text>{width()}</Text>
    </View>
  );
}
