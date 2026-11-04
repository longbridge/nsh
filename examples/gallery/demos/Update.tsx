import { openUrl, signal, Text } from "nsh";
import { checkForUpdate, downloadUpdate, installAndRestart } from "nsh/update";
import type { AppUpdate } from "nsh/update";
import { Button, Card, colors, DemoPage } from "../ui";

/**
 * nsh/update: asks the source in nsh.json's "update" for a newer version,
 * then downloads it and replaces the app, or sends the user to its link
 * where the app cannot replace itself. The gallery names no source, so here
 * the check answers that there is nothing to install.
 */
export function UpdateDemo() {
  const status = signal("Not checked yet");
  const update = signal<AppUpdate | null>(null);

  const check = async () => {
    status.set("Checking…");
    update.set(null);
    try {
      const found = await checkForUpdate();
      update.set(found);
      status.set(found ? `Version ${found.version} is available` : "No update available");
    } catch (error) {
      status.set(`Check failed: ${(error as Error).message}`);
    }
  };

  const install = async () => {
    const found = update();
    if (!found) return;
    if (!found.canInstall) {
      await openUrl(found.url);
      return;
    }
    try {
      await downloadUpdate(({ received, total }) =>
        status.set(total ? `Downloading ${Math.round((received / total) * 100)}%` : `Downloading ${received} bytes`),
      );
      status.set("Installing…");
      await installAndRestart();
    } catch (error) {
      status.set(`Update failed: ${(error as Error).message}`);
    }
  };

  function Status() {
    return (
      <Card icon="system" title="App update" subtitle="From GitHub Releases or a host of your own">
        <Text style={{ fontSize: 14, color: colors.text }}>{status()}</Text>
        <Button label="Check for updates" onPress={check} />
        {update() ? <Button label="Install" onPress={install} /> : null}
      </Card>
    );
  }

  return <DemoPage cards={[Status]} />;
}
