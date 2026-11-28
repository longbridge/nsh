// Menu wraps `nsh/base`'s Menu, MenuItem, Submenu and MenuSeparator. The
// elements own placement at the trigger, arrow keys, typeahead, Escape,
// dismissal and the cascade: a Submenu opens beside its row on desktop and
// pushes in place on phones (its `back` row). Never list nested items inline,
// and never use MenuButton for a drop-down attached to a trigger.
import { Text, View, token, type JSX } from "nsh";
import { Menu as BaseMenu, MenuItem as BaseItem, MenuSeparator as BaseSeparator, Submenu as BaseSubmenu } from "nsh/base";
import { Icon, type IconName } from "./icon";

const popup = "min-w-48 rounded-lg border border-border bg-popover p-1 shadow-md";
const row = "h-8 flex-row items-center gap-2 rounded-md px-2 hover:bg-accent focus:bg-accent disabled:opacity-50";

/** `trigger` is usually an `IconButton` or a `Button`. */
export function Menu(props: { trigger: JSX.Element; children: JSX.Element }) {
  return (
    <BaseMenu offset={4} popupClass={popup} backdropClass="bg-black/40" trigger={props.trigger}>
      {props.children}
    </BaseMenu>
  );
}

export function MenuItem(props: {
  icon?: IconName;
  shortcut?: string;
  destructive?: boolean;
  disabled?: boolean;
  onSelect: () => void;
  children: string;
}) {
  const ink = () => (props.destructive ? "text-destructive" : "text-popover-foreground");
  return (
    <BaseItem class={row} disabled={props.disabled} onSelect={props.onSelect}>
      {props.icon ? <Icon name={props.icon} size={14} color={token(props.destructive ? "destructive" : "popover-foreground")} /> : <View class="w-3.5" />}
      <Text class={`flex-1 text-sm ${ink()}`}>{props.children}</Text>
      {props.shortcut && <Text class="text-xs text-muted-foreground">{props.shortcut}</Text>}
    </BaseItem>
  );
}

/** A row that opens a nested menu beside it. */
export function MenuSub(props: { icon?: IconName; title: string; children: JSX.Element }) {
  return (
    <BaseSubmenu
      offset={4}
      trigger={
        <View class={row}>
          {props.icon ? <Icon name={props.icon} size={14} /> : <View class="w-3.5" />}
          <Text class="flex-1 text-sm text-popover-foreground">{props.title}</Text>
          <Icon name="chevron-right" size={14} color={token("muted-foreground")} />
        </View>
      }
      back={
        <View class={row}>
          <Icon name="chevron-left" size={14} />
          <Text class="text-sm text-popover-foreground">{props.title}</Text>
        </View>
      }
    >
      {props.children}
    </BaseSubmenu>
  );
}

export function MenuSeparator() {
  return <BaseSeparator class="my-1 h-px bg-border" />;
}
