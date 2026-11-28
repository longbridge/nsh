// A gallery of the reference `ui` layer (ui/): every control wraps its
// `nsh/base` element. Run it with `nsh dev examples/ui-kit`.
import { mount, signal, Text, View } from "nsh";
import { Window } from "nsh/desktop";
import { Button, IconButton, Menu, MenuItem, MenuSeparator, MenuSub, Select, TabPanel, Tabs } from "../../skills/nsh/references/ui-kit";

const regions = [
  { value: "us", title: "United States" },
  { value: "hk", title: "Hong Kong" },
  { value: "sg", title: "Singapore" },
  { value: "cn", title: "Mainland China", disabled: true },
];

function App() {
  const tab = signal("general");
  const region = signal<string | null>(null);
  const last = signal("nothing yet");

  return (
    <View class="flex-1 gap-6 bg-background p-6">
      <View class="flex-row items-center justify-between">
        <Text class="text-lg font-semibold text-foreground">Settings</Text>
        <Menu trigger={<IconButton icon="ellipsis" label="More actions" />}>
          <MenuItem icon="pencil" shortcut="⌘R" onSelect={() => last.set("Rename")}>Rename</MenuItem>
          <MenuItem icon="copy" onSelect={() => last.set("Duplicate")}>Duplicate</MenuItem>
          <MenuSub icon="share" title="Share">
            <MenuItem onSelect={() => last.set("Copy link")}>Copy link</MenuItem>
            <MenuItem onSelect={() => last.set("Email")}>Email</MenuItem>
          </MenuSub>
          <MenuSeparator />
          <MenuItem icon="trash" destructive onSelect={() => last.set("Delete")}>Delete</MenuItem>
        </Menu>
      </View>

      <Tabs
        label="Sections"
        value={tab()}
        onChange={(v) => tab.set(v)}
        items={[
          { value: "general", title: "General", icon: "settings" },
          { value: "account", title: "Account" },
        ]}
      />

      <TabPanel value="general" selected={tab()}>
        <View class="gap-2">
          <Text class="text-sm font-medium text-foreground">Region</Text>
          <Select
            label="Region"
            placeholder="Choose a region"
            items={regions}
            value={region()}
            onChange={(v) => region.set(v)}
            width={240}
          />
        </View>
      </TabPanel>
      <TabPanel value="account" selected={tab()}>
        <Text class="text-sm text-muted-foreground">Signed in as demo@example.com</Text>
      </TabPanel>

      <View class="flex-row gap-2">
        <Button variant="primary" icon="check" onPress={() => last.set("Save")}>Save</Button>
        <Button variant="ghost" onPress={() => last.set("Cancel")}>Cancel</Button>
        <IconButton icon="plus" label="Add" variant="secondary" onPress={() => last.set("Add")} />
      </View>
      <Text class="text-sm text-muted-foreground">Last action: {last()}</Text>
    </View>
  );
}

mount(() => <Window id="main" title="UI Kit">{App}</Window>);
