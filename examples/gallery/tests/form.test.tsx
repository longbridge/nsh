// Inputs: typing goes key by key, so a controlled input's own value and
// `maxLength` apply as on a device.
import { expect, open, test } from "nsh/test";

test("a controlled input shows what the app makes of each key", async () => {
  const app = await open("../App.tsx");
  await app.press("label:Form");
  await app.type("id:shout", "hello");
  expect(app.value("id:shout")).toBe("HELLO");
});

test("an input stops at its maxLength", async () => {
  const app = await open("../App.tsx");
  await app.press("label:Form");
  await app.type("id:code", "12345678");
  expect(app.value("id:code")).toBe("123456");
  await app.waitFor("6/6");
});
