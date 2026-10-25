// A case of the API's test suite (docs/dev/compatibility.md): `DiffView`
// of `nsh/base`, the changes between two texts the app owns (`oldValue`,
// `newValue`): the unchanged lines beyond `contextLines` around each
// change collapse into separators the app draws (`renderSeparator`, told
// the gap and how many lines), which the ref expands (`expand`); each
// change carries the app's actions (`renderHunkActions`, told the hunk),
// which revert or keep it through the ref (`revertHunk`, `acceptHunk`),
// reported with both texts (`onChange`) for the app to set; the ref
// reads the changes (`hunks`) and their counts (`stats`). A unified
// `patch` shows its hunks under their headers.
import { Text, View, signal } from "nsh";
import { DiffView, type DiffViewRef } from "nsh/base";

const OLD = "a\nb\nc\nd\ne\nf\ng\nh\n";
const NEW = "a\nB\nc\nd\ne\nf\nG\nh\n";
const PATCH = `--- a/main.rs
+++ b/main.rs
@@ -1,3 +1,3 @@ fn main
 one
-two
+TWO
 three
`;

export default function BaseDiff() {
  const oldValue = signal(OLD);
  const newValue = signal(NEW);
  const read = signal("none");
  let diff!: DiffViewRef;
  const inspect = async () => {
    const stats = await diff.stats();
    const hunks = (await diff.hunks()).map(h => `${h.index}@${h.oldStart}:${h.oldLines}>${h.newStart}:${h.newLines}`);
    read.set(`+${stats.added} -${stats.deleted} ${stats.hunks} [${hunks.join(" ")}]`);
  };
  return (
    <View style={{ paddingTop: 20 }}>
      <View style={{ width: 390, height: 240 }}>
        <DiffView
          ref={r => (diff = r)}
          oldValue={oldValue()}
          newValue={newValue()}
          layout="unified"
          contextLines={1}
          renderSeparator={sep => <Text>{`Hidden ${sep.gap} ${sep.lines}`}</Text>}
          renderHunkActions={hunk => (
            <View style={{ flexDirection: "row" }}>
              <View testID={`revert-${hunk.index}`} onPress={() => diff.revertHunk(hunk.index)}>
                <Text>{`Revert ${hunk.index}`}</Text>
              </View>
              <View testID={`keep-${hunk.index}`} onPress={() => diff.acceptHunk(hunk.index)}>
                <Text>{`Keep ${hunk.index}`}</Text>
              </View>
            </View>
          )}
          onChange={change => {
            oldValue.set(change.oldValue);
            newValue.set(change.newValue);
          }}
          style={{ flex: 1 }}
        />
      </View>
      <View style={{ width: 390, height: 120 }}>
        <DiffView
          patch={PATCH}
          renderSeparator={sep => <Text>{sep.header ? `Header ${sep.header.trim()}` : `Gap ${sep.lines}`}</Text>}
          style={{ flex: 1 }}
        />
      </View>
      <Text>{() => `Old: ${oldValue().trim().split("\n").join(" ")}`}</Text>
      <Text>{() => `New: ${newValue().trim().split("\n").join(" ")}`}</Text>
      <Text>{() => `Read: ${read()}`}</Text>
      <View testID="inspect" style={{ height: 20 }} onPress={inspect} />
      <View testID="expand" style={{ height: 20 }} onPress={() => diff.expand(1)} />
    </View>
  );
}
