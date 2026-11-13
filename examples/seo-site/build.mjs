#!/usr/bin/env node
// Static site generation, kept outside NSH: writes each page of the site as
// static HTML when the site is built. It renders every URL with `nsh render`
// (no browser) against the API, and writes it into the packaged page.
//
//   nsh package examples/seo-site --target web -o dist/site
//   node build.mjs <nsh> <site dir> [--url https://notes.example.com/]
//
// The pages are the home page and one per note (the API's list); the site
// directory then holds `index.html` and `notes/<slug>/index.html`, which any
// static host serves. The app takes over in the browser with the data the
// page was written with (`host.params`) and reads the API again.

import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { listen } from "./api.mjs";
import { writePage } from "./write-page.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const siteURL = option("url", "/");
const [nsh, site] = args;
if (!nsh || !site) {
  console.error("usage: node build.mjs <nsh> <site dir written by nsh package --target web> [--url https://notes.example.com/]");
  process.exit(2);
}
const app = new URL(".", import.meta.url).pathname;
// The packaged page every page is written into: kept beside the pages the
// first time, since the home page is written over `index.html`.
const packaged = join(site, "index.template.html");
if (!existsSync(packaged)) copyFileSync(join(site, "index.html"), packaged);
const template = readFileSync(packaged, "utf8");

// The API the pages read, on a port of its own while the pages render.
const server = await listen();
const origin = `http://127.0.0.1:${server.address().port}`;

/** One page, light and dark. */
const run = promisify(execFile);
const render = async (path, appearance = "light") => {
  // Not a synchronous call: this process answers the API meanwhile.
  const { stdout: out } = await run(nsh, ["render", app, "--path", path, "--base", `${origin}/`, "--appearance", appearance], {
    maxBuffer: 64 << 20,
  });
  const shot = JSON.parse(out);
  if (shot.error) throw new Error(`${path}: ${shot.error}`);
  return shot;
};

const notes = await (await fetch(`${origin}/api/notes`)).json();
const paths = ["/", ...notes.map(note => `/notes/${note.slug}`)];
for (const path of paths) {
  const started = Date.now();
  const [shot, dark] = await Promise.all([render(path), render(path, "dark")]);
  const html = writePage(template, { ...shot, dark }, path, { origin, siteURL });
  const file = join(site, path === "/" ? "index.html" : `${path.slice(1)}/index.html`);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, html);
  console.log(`  ${path} (${Date.now() - started} ms)`);
}
server.close();
