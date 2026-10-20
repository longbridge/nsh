import { signal, Text } from "nsh";
import { Platform } from "nsh/device";
import console from "console";
import os from "os";
import process from "process";
import { Button, Card, colors, DemoPage, lazy, mono } from "../ui";

const Native = lazy(() => import("./StandardNative").then((m) => m.loadStandardNative()));

/** Standard modules are loaded when their demo is opened. */
export function Standard() {
  const tick = signal("Ready");
  function Environment() {
    let home = "Browser context";
    if (Platform.OS !== "web") {
      try {
        home = os.homedir();
      } catch {
        home = "No native home directory configured";
      }
    }
    return <Card icon="system" title="Host and jobs" subtitle="os · process · console · import.meta.url">
      <Text selectable style={{ color: colors.text, fontFamily: mono, fontSize: 13 }}>{`${os.platform() || Platform.OS} / ${os.arch()}\nHome: ${home}\nData: ${process.dataDir}\nModule: ${import.meta.url}\nEOL: ${JSON.stringify(os.EOL)}\nEnvironment: ${JSON.stringify(process.env)}`}</Text>
      <Button label="Schedule nextTick" onPress={() => {
        tick.set("Queued");
        process.nextTick((value: string) => { tick.set(value); console.info("Gallery nextTick", value); }, "nextTick delivered");
      }} />
      <Text style={{ color: colors.muted }}>{tick()}</Text>
    </Card>;
  }
  if (Platform.OS === "web") return <DemoPage cards={[Environment]} />;
  return <Native />;
}

export { Standard as StandardDemo };
