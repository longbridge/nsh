// The demo's API (notes and made-up prices), kept outside NSH: what the site's
// pages read at build time, and what the published site reads when it runs.

import { createServer } from "node:http";

import { notes, prices } from "./data.mjs";

const summary = ({ body, ...note }) => note;
let tick = 0;
function quotes(symbols) {
  tick++;
  return symbols.filter(s => s in prices).map(symbol => {
    const open = prices[symbol];
    // A made-up walk: a new price on every request.
    const price = open * (1 + 0.02 * Math.sin(tick / 3 + symbol.length) + 0.004 * (Math.random() - 0.5));
    return { symbol, price: Math.round(price * 100) / 100, change: Math.round((price / open - 1) * 10000) / 100, at: new Date().toISOString() };
  });
}

/** Answers `/api/…`; false for any other path. */
export function api(req, res, url) {
  if (!url.pathname.startsWith("/api/")) return false;
  const json = (status, value) => {
    res.writeHead(status, { "content-type": "application/json", "cache-control": "no-store" });
    res.end(JSON.stringify(value));
  };
  if (url.pathname === "/api/notes") return json(200, notes.map(summary)), true;
  if (url.pathname === "/api/quotes") return json(200, quotes((url.searchParams.get("symbols") ?? "").split(","))), true;
  const note = notes.find(n => `/api/notes/${n.slug}` === url.pathname);
  return note ? json(200, note) : json(404, { error: "no such note" }), true;
}

/** The API alone on a port, for the build to render against. */
export function listen(port = 0) {
  const server = createServer((req, res) => {
    if (!api(req, res, new URL(req.url, "http://localhost"))) { res.writeHead(404); res.end(); }
  });
  return new Promise(resolve => server.listen(port, "127.0.0.1", () => resolve(server)));
}
