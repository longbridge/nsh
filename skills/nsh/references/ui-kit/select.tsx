// Select wraps `nsh/base`'s Select. The element owns the popup (as wide as
// the trigger on desktop and web, a sheet on phones), keyboard navigation,
// typeahead, dismissal and accessibility. This file only draws the trigger
// (with a chevron icon, never a ▾ character) and the popup's rows.
import { Text, View } from "nsh";
import { Select as BaseSelect, type SelectItem } from "nsh/base";
import { Icon } from "./icon";

export type { SelectItem };

export function Select(props: {
  items: SelectItem[];
  value: string | null;
  onChange: (value: string | null) => void;
  label: string;
  placeholder?: string;
  disabled?: boolean;
  /** The trigger's width; the popup follows it. Defaults to filling its row. */
  width?: number;
}) {
  const title = () => props.items.find((i) => i.value === props.value)?.title;
  return (
    <BaseSelect
      items={props.items}
      value={props.value}
      onChange={({ value }) => props.onChange(value)}
      accessibilityLabel={props.label}
      placeholder={props.placeholder}
      disabled={props.disabled}
      offset={4}
      style={props.width ? { width: props.width } : undefined}
      popupClass="rounded-lg border border-border bg-popover p-1 shadow-md"
      backdropClass="bg-black/40"
      itemClass="h-8 rounded-md px-2 text-sm text-popover-foreground focus:bg-accent hover:bg-accent disabled:opacity-50"
      trigger={
        <View class="h-9 flex-row items-center justify-between gap-2 rounded-md border border-input bg-background px-3 hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring">
          <Text class={`text-sm ${title() ? "text-foreground" : "text-muted-foreground"}`}>
            {title() ?? props.placeholder ?? ""}
          </Text>
          <Icon name="chevron-down" size={16} />
        </View>
      }
    />
  );
}
