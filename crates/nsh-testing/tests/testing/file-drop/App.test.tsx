import { expect, open, test } from "nsh/test";

// longbridge/nsh#2478: the dropped file is user-approved, as a picked one
// is; the harness wrote /tmp/nsh-file-drop-probe.md holding `# Hallo`, and
// this app's nsh.json grants reads of `${dataDir}` only.
test("a dropped file can be read", async () => {
  const app = await open("./App.tsx", { platform: "macos" });
  await app.waitFor("id:out");
  await app.dropFiles("id:area", ["file:///tmp/nsh-file-drop-probe.md"]);
  await app.waitFor("text:ok:# Hallo");
  expect(app.texts()).toContain("ok:# Hallo");
});
