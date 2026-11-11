// A case of the API's test suite (docs/dev/compatibility.md): `Tooltip`'s
// `gracePeriod` on a desktop. Once the pointer left a trigger whose tooltip
// showed, a trigger it comes to within that trigger's tooltip's grace
// period shows its tooltip at once, without its `delay` (a minute here,
// longer than a script waits); past the default one (300 ms), a trigger
// waits for its delay. Its script checks each.
import { Text, View } from "nsh";
import { Tooltip } from "nsh/base";

function App() {
  return (
    <View style={{ flex: 1, paddingTop: 300, paddingLeft: 20, gap: 8 }}>
      <View style={{ height: 30, width: 100 }} tooltip={<Tooltip delay={500}><Text>FirstTip</Text></Tooltip>}>
        <Text>FirstTrigger</Text>
      </View>
      <View
        style={{ height: 30, width: 100 }}
        tooltip={
          <Tooltip delay={60000} gracePeriod={4000}>
            <Text>GraceTip</Text>
          </Tooltip>
        }
      >
        <Text>GraceTrigger</Text>
      </View>
      <View style={{ height: 30, width: 100 }} tooltip={<Tooltip delay={60000}><Text>WaitTip</Text></Tooltip>}>
        <Text>WaitTrigger</Text>
      </View>
    </View>
  );
}

export default App;
