// fetch() against responses the test lists: no request reaches the network.
import { expect, open, test } from "nsh/test";

const POSTS = "GET https://jsonplaceholder.typicode.com/posts?_limit=3";

test("loads the posts the server answers with", async () => {
  const app = await open("../App.tsx", {
    network: { [POSTS]: { body: [{ id: 1, title: "Hello from the test" }] } },
  });
  await app.press("label:System");
  await app.press("fetch()");
  await app.waitFor("Hello from the test");
  expect(app.texts()).toContain("#1");
});
