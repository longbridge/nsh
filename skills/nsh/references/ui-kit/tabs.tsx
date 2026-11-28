// Tabs wraps `nsh/base`'s Tabs and Tab: arrow-key movement, the tab role and
// the sliding indicator come from the elements. `nsh/base` leaves the value
// to the app, so this wrapper holds it: pass `value` and `onChange`, and
// render the panel for the value next to it.
import { Text, type JSX } from "nsh";
import { Tab, Tabs as BaseTabs } from "nsh/base";
import { Icon, type IconName } from "./icon";

export interface TabItem {
  value: string;
  title: string;
  icon?: IconName;
}

export function Tabs(props: { items: TabItem[]; value: string; onChange: (value: string) => void; label: string }) {
  return (
    <BaseTabs
      accessibilityLabel={props.label}
      class="flex-row self-start gap-1 rounded-lg bg-muted p-1"
      indicatorClass="inset-y-1 rounded-md bg-background shadow-sm"
    >
      {props.items.map((item) => (
        <Tab
          selected={props.value === item.value}
          onPress={() => props.onChange(item.value)}
          class="group h-7 flex-row items-center gap-1.5 rounded-md px-3 focus-visible:ring-2 focus-visible:ring-ring"
        >
          {item.icon && <Icon name={item.icon} size={14} />}
          <Text class="text-sm font-medium text-muted-foreground group-selected:text-foreground">{item.title}</Text>
        </Tab>
      ))}
    </BaseTabs>
  );
}

/** The content under a `Tabs`: shows `children` for the selected value only. */
export function TabPanel(props: { value: string; selected: string; children: JSX.Element }) {
  return <>{props.value === props.selected ? props.children : null}</>;
}
