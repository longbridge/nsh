// `DockArea`'s `scrollableTabs`: a tab bar's tabs in a horizontal scroll
// view when they do not fit (eight tabs of 80 in an area 390 wide), which
// scrolls and brings the displayed tab into view.
import { Text, View } from "nsh";
import { DockArea, type DockAreaRef } from "nsh/base";

export default function BaseDockScrollableTabs() {
  let area: DockAreaRef | undefined;
  return (
    <View style={{ paddingTop: 20 }}>
      <View testID="strip-show-h" onPress={() => area?.selectPanel("h")}><Text>Show h</Text></View>
      <View style={{ width: 390, height: 200 }}>
        <DockArea
          ref={r => (area = r)}
          compact={false}
          scrollableTabs
          defaultLayout={{ center: { type: "tabs", panels: ["a", "b", "c", "d", "e", "f", "g", "h"] } }}
          class="flex-1"
          tabBarClass="flex-row h-[24px]"
          tabClass="w-[80px]"
          renderTab={tab => <Text testID={`strip-tab-${tab.panel}`}>{`Strip ${tab.panel}${tab.active ? "*" : ""}`}</Text>}
          renderPanel={panel => <Text>{`Strip panel ${panel.id}`}</Text>}
        />
      </View>
    </View>
  );
}
