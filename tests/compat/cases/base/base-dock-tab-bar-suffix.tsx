// `DockArea`'s `renderTabBarSuffix`: what follows a tab group's tabs in
// its tab bar, given the group (its panels, its displayed one, its region).
import { Text, View } from "nsh";
import { DockArea } from "nsh/base";

export default function BaseDockTabBarSuffix() {
  return (
    <View style={{ paddingTop: 20 }}>
      <View style={{ width: 390, height: 200 }}>
        <DockArea
          compact={false}
          defaultLayout={{
            center: {
              type: "split",
              axis: "horizontal",
              children: [
                { type: "tabs", panels: ["a", "b"] },
                { type: "tabs", panels: ["c"] },
              ],
            },
          }}
          class="flex-1"
          tabBarClass="flex-row h-[24px]"
          renderTab={tab => <Text testID={`suffix-tab-${tab.panel}`}>{`Tab ${tab.panel}`}</Text>}
          renderPanel={panel => <Text>{`Panel ${panel.id}`}</Text>}
          renderTabBarSuffix={group => (
            <Text>{`Suffix ${group.placement} ${group.panels.join(",")} shows ${group.active}`}</Text>
          )}
        />
      </View>
    </View>
  );
}
