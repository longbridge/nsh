// Intrinsic document extent and a completed page-coordinate refresh reach
// JS through the generated RichEditor event and session method.
import { Text, View, signal } from "nsh";
import { RichEditor, createRichEditor } from "nsh/base";

export default function App() {
  const editor = createRichEditor({ content: "First line\n\nLast line", format: "markdown" });
  const moved = signal(false);
  const extent = signal("Extent: waiting");
  const placement = signal("Placement: waiting");
  let firstY: number | null = null;
  return (
    <View style={{ flex: 1, paddingTop: 60 }}>
      <View testID="move-editor" onPress={() => {
        moved.set(true);
        editor.refreshGeometry();
      }}><Text>Move enclosing layout</Text></View>
      <View style={{ height: moved() ? 75 : 0, flexShrink: 0 }} />
      <RichEditor editor={editor} style={{ height: 100, flexShrink: 0 }} onGeometryChange={({ geometry }) => {
        const size = geometry.contentSize;
        extent.set(`Extent: ${size.width > 0} ${size.height > 0 && size.height < 100} ${geometry.scrollOffset === 0} ${typeof geometry.generation === "number"} ${geometry.documentRevision === 0}`);
        const rect = editor.pageCoordsAtPos(1) as { y: number } | null;
        if (!moved() && rect) firstY = rect.y;
        if (moved() && rect && firstY !== null) {
          placement.set(`Placement: ${Math.abs(rect.y - firstY - 75) < 1}`);
        }
      }} />
      <Text>{extent()}</Text>
      <Text>{placement()}</Text>
    </View>
  );
}
