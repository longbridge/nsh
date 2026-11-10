import { mount, signal, Text, token, View } from "nsh";
import { Button } from "nsh/base";
import { Workspace, type WorkspacePalette } from "rust-exports/native";

function App() {
  const selected = signal(Workspace.current());
  const immediate = signal("Choose a workspace");

  const choose = (palette: WorkspacePalette) => {
    Workspace.select(palette);
    selected.set(Workspace.current());
    immediate.set(`Read at once: ${token("workspace-accent")}`);
  };

  return (
    <View class="flex-1 bg-background p-6 gap-4">
      <Text class="text-2xl font-semibold text-foreground">Workspace palettes</Text>
      <Text class="text-muted-foreground">Each workspace uses the app's theme plugin.</Text>
      <Text class="text-workspace-accent text-xl">Workspace preview</Text>
      <Text class="text-foreground">{`Workspace: ${selected()}`}</Text>
      <Text class="text-foreground">{`Accent: ${token("workspace-accent")}`}</Text>
      <Text class="text-muted-foreground">{immediate()}</Text>
      <View class="flex-row gap-3">
        <Button class="rounded-lg bg-secondary px-4 py-2" onPress={() => choose("ocean")}>
          <Text class="text-secondary-foreground">Ocean</Text>
        </Button>
        <Button class="rounded-lg bg-secondary px-4 py-2" onPress={() => choose("orchid")}>
          <Text class="text-secondary-foreground">Orchid</Text>
        </Button>
        <Button class="rounded-lg bg-secondary px-4 py-2" onPress={() => choose("built")}>
          <Text class="text-secondary-foreground">Built theme</Text>
        </Button>
      </View>
    </View>
  );
}

mount(App);
