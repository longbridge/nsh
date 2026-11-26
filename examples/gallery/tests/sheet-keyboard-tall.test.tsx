import { expect, open, test } from "nsh/test";

// #3288: one sheet, the same code on iOS and Android, keeps its header on
// screen with the keyboard up and is lifted once when padded by the
// keyboard inset.
for (const mode of ["tall", "padded"]) {
  test(`a ${mode} sheet stays on screen above the keyboard`, async () => {
    const app = await open("./fixtures/sheet-keyboard-tall.tsx");
    // Only a native phone shows a software keyboard.
    if (!app.shows("Platform: ios") && !app.shows("Platform: android")) return;
    await app.press(`id:open-${mode}`);
    await app.waitFor("id:close");
    await app.type("id:title", "A title");
    await app.waitFor("text:keyboard: up");
    await app.host.send("test:measure");
    await app.waitFor("text:layout: ok");
    expect(app.shows("id:close")).toBe(true);
    await app.press("id:close");
    await app.waitFor("text:sheet: closed");
    // Opened again, it is laid out the same way.
    await app.press(`id:open-${mode}`);
    await app.waitFor("id:close");
    await app.type("id:title", "Again");
    await app.waitFor("text:keyboard: up");
    await app.host.send("test:measure");
    await app.waitFor("text:layout: ok");
  });
}
