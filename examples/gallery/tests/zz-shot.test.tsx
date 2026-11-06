import { open, test } from "nsh/test";
for (const scheme of ["dark", "light"] as const) {
  test(`shot ${scheme}`, async () => {
    const app = await open("../App.tsx", { colorScheme: scheme });
    await app.press("role:button[Terminal]");
    await app.waitFor("Welcome to the NSH terminal.");
    for (const cmd of ["colors", "progress"]) {
      await app.type("label:Terminal", cmd);
      await app.keyboard.press("enter");
      await app.wait(1500);
    }
    await app.screenshot(`/tmp/term-${scheme}.png`);
  });
}
