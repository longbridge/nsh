// Button and IconButton wrap `nsh/base`'s Button: focus, keyboard activation,
// the busy state and accessibility come from it; this file adds the look.
import { Text, token, type JSX } from "nsh";
import { Button as BaseButton } from "nsh/base";
import { Icon, type IconName } from "./icon";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "destructive";
export type ButtonSize = "sm" | "md";

const surface: Record<ButtonVariant, string> = {
  primary: "bg-primary hover:bg-primary/90 active:bg-primary/80",
  secondary: "bg-secondary hover:bg-secondary/80 active:bg-secondary/70",
  ghost: "hover:bg-accent active:bg-accent/80",
  destructive: "bg-destructive hover:bg-destructive/90 active:bg-destructive/80",
};
const ink: Record<ButtonVariant, "primary-foreground" | "secondary-foreground" | "accent-foreground" | "foreground"> = {
  primary: "primary-foreground",
  secondary: "secondary-foreground",
  ghost: "foreground",
  destructive: "primary-foreground",
};
const label: Record<ButtonVariant, string> = {
  primary: "text-primary-foreground",
  secondary: "text-secondary-foreground",
  ghost: "text-foreground",
  destructive: "text-primary-foreground",
};
const box: Record<ButtonSize, string> = { sm: "h-7 px-2.5 gap-1.5 rounded-md", md: "h-9 px-3.5 gap-2 rounded-md" };
const iconBox: Record<ButtonSize, string> = { sm: "size-7 rounded-md", md: "size-9 rounded-md" };
const iconSize: Record<ButtonSize, number> = { sm: 14, md: 16 };

const base =
  "flex-row items-center justify-center focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";

export function Button(props: {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: IconName;
  disabled?: boolean;
  busy?: boolean;
  onPress?: () => void;
  testID?: string;
  children: JSX.Element;
}) {
  const v = () => props.variant ?? "secondary";
  const s = () => props.size ?? "md";
  return (
    <BaseButton
      class={`${base} ${box[s()]} ${surface[v()]}`}
      disabled={props.disabled}
      busy={props.busy}
      onPress={props.onPress}
      testID={props.testID}
    >
      {props.icon && <Icon name={props.icon} size={iconSize[s()]} color={token(ink[v()])} />}
      <Text class={`text-sm font-medium ${label[v()]}`}>{props.children}</Text>
    </BaseButton>
  );
}

/** An icon-only button. `label` is required: it is the accessibility label and the test handle. */
export function IconButton(props: {
  icon: IconName;
  label: string;
  variant?: ButtonVariant;
  size?: ButtonSize;
  disabled?: boolean;
  onPress?: () => void;
}) {
  const v = () => props.variant ?? "ghost";
  const s = () => props.size ?? "md";
  return (
    <BaseButton
      class={`${base} ${iconBox[s()]} ${surface[v()]}`}
      accessibilityLabel={props.label}
      disabled={props.disabled}
      onPress={props.onPress}
    >
      <Icon name={props.icon} size={iconSize[s()]} color={token(ink[v()])} />
    </BaseButton>
  );
}
