// Writes a page of the site: the packaged page (`nsh package --target web`)
// with what `nsh render` made of a URL in it, kept outside NSH.

const escapeText = text => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const escapeAttribute = text => escapeText(text).replace(/"/g, "&quot;");

/**
 * The packaged page `template` with `shot` written in, for `path`:
 * `siteURL` is where the site is published, which absolute URLs the app
 * wrote (a canonical link) point to instead of the renderer's `origin`.
 */
export function writePage(template, shot, path, { origin, siteURL = "/" }) {
  const depth = path.split("/").filter(Boolean).length;
  const base = depth ? "../".repeat(depth) : "./";
  const head = shot.head.split(`${origin}/`).join(siteURL);
  // The page in the dark appearance (`shot.dark`), for visitors whose
  // system is dark: kept in a template (not shown, not the page's text) and
  // swapped in by a script that runs before the page first paints. Its
  // classes are renamed so they do not meet the light page's.
  let dark = "", darkCss = "";
  if (shot.dark) {
    const rename = text => text.replace(/nsh-s(\d+)/g, "nsh-d$1");
    darkCss = rename(shot.dark.css);
    dark = `<template id="nsh-dark">${rename(shot.dark.html)}</template>\n<script>if (matchMedia("(prefers-color-scheme: dark)").matches) ` +
      `document.getElementById("prerendered").replaceChildren(document.getElementById("nsh-dark").content.cloneNode(true));</script>`;
  }
  // Embedded JSON: nothing in it may close the script.
  const params = JSON.stringify({ fetches: shot.fetches }).replace(/</g, "\\u003c");
  // What is written in comes from the app and its API: a function gives it
  // as it is, where a replacement string would read its `$&`, `$'` and `$$`
  // as patterns.
  const put = (page, pattern, text) => page.replace(pattern, () => text);
  let page = put(template, "<head>", `<head>\n<base href="${base}">`);
  page = put(page, /<html lang="[^"]*">/, `<html lang="${escapeAttribute(shot.lang || "en")}">`);
  // A page without a `<Head>` keeps the packaged page's title.
  if (shot.title != null) page = put(page, /<title>[\s\S]*?<\/title>/, `<title>${escapeText(shot.title)}</title>`);
  page = put(page, "</head>", `${head}\n<style data-prerendered>\n${(shot.css + darkCss).replace(/<\/style/gi, "<\\/style")}</style>\n` +
    `<style>#prerendered { position: fixed; inset: 0; z-index: 1; }</style>\n` +
    `<script type="application/json" id="nsh-params">${params}</script>\n</head>`);
  page = put(page, '<div id="status"', '<div id="status" hidden');
  // The written page shows the content without JavaScript.
  page = put(page, /<noscript>[\s\S]*?<\/noscript>/, "");
  return put(page, '<div id="app"></div>', `<div id="prerendered">${shot.html}</div>\n${dark}\n<div id="app"></div>`);
}
