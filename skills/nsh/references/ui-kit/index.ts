// The app's `ui` layer. Feature code imports controls from here, never from
// `nsh/base`. Copy this folder into `src/ui/`, then change the look (classes,
// sizes, icons) to the app's design system; keep each wrapper on its element.
export { Button, IconButton, type ButtonSize, type ButtonVariant } from "./button";
export { Icon, type IconName } from "./icon";
export { Menu, MenuItem, MenuSeparator, MenuSub } from "./menu";
export { Select, type SelectItem } from "./select";
export { TabPanel, Tabs, type TabItem } from "./tabs";
