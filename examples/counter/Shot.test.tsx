import { expect, open, test } from "nsh/test";

test("screenshot", async () => {
  const app = await open("./App.tsx");
  await app.screenshot("/tmp/T10661-shot.png");
  expect(true).toBe(true);
});
