import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import Slugger from 'github-slugger';
import { fromMarkdown } from 'mdast-util-from-markdown';
import { toString } from 'mdast-util-to-string';

// A translated page gives its headings the English page's ids, so a copied
// section link is short, readable and the same in every language
// (`/zh-CN/docs/fonts#fallbacks`, not `#%E5%9B%9E%E9%80%80…`). Headings pair up
// by position: a translation keeps the English outline (`tests/content.test.ts`
// checks the Chinese pages). Links to a section of a translated page, written
// with the translated heading (`./text.md#文字样式`, `#回退列表`), are rewritten
// to the English id too, so existing pages and machine translations keep working.

const SITE_ROOT = resolve(process.cwd());
const SECTIONS = new Set(['docs', 'elements', 'modules']);

/** `{ locale, section, name }` for a translated page, `undefined` for an English one. */
export function translatedPage(path) {
  const parts = relative(SITE_ROOT, resolve(path)).split(sep);
  if (parts[0] === 'zh-CN' && parts.length === 3 && SECTIONS.has(parts[1])) {
    return { locale: 'zh-CN', section: parts[1], name: parts[2] };
  }
  if (parts[0] === 'translations' && parts.length === 4 && SECTIONS.has(parts[2])) {
    return { locale: parts[1], section: parts[2], name: parts[3] };
  }
  return undefined;
}

function stripFrontmatter(source) {
  return source.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
}

/**
 * The headings of a Markdown source, h1 title dropped as `remarkStripTitle`
 * does, each with the id Astro's `rehypeHeadingIds` would give it.
 */
export function headingsOf(source) {
  const tree = fromMarkdown(stripFrontmatter(source));
  const first = tree.children.findIndex((n) => n.type !== 'html');
  if (first !== -1 && tree.children[first].type === 'heading' && tree.children[first].depth === 1) {
    tree.children.splice(first, 1);
  }
  const slugger = new Slugger();
  const out = [];
  const walk = (node) => {
    if (node.type === 'heading') out.push({ depth: node.depth, slug: slugger.slug(toString(node)), node });
    else for (const child of node.children ?? []) walk(child);
  };
  walk(tree);
  return out;
}

const cache = new Map();

function read(path) {
  return existsSync(path) ? readFileSync(path, 'utf8') : undefined;
}

/**
 * For a translated page: its own heading ids in order and the English id at
 * each position. Pairing stops at the first heading whose level differs, so a
 * page whose outline drifted keeps its own ids past that point.
 */
export function idMap(path) {
  const key = resolve(path);
  if (cache.has(key)) return cache.get(key);
  let result;
  const page = translatedPage(key);
  const own = page && read(key);
  const english = page && read(join(SITE_ROOT, page.section, page.name));
  if (own !== undefined && english !== undefined) {
    const mine = headingsOf(own);
    const theirs = headingsOf(english);
    const ids = [];
    for (let i = 0; i < mine.length; i++) {
      if (i < theirs.length && theirs[i].depth === mine[i].depth && ids.length === i) ids.push(theirs[i].slug);
      else break;
    }
    result = { own: mine.map((h) => h.slug), ids };
  }
  cache.set(key, result);
  return result;
}

/** The English id for `fragment` (a translated heading's slug) on the page at `path`. */
function translate(path, fragment) {
  const map = idMap(path);
  if (!map) return fragment;
  let decoded = fragment;
  try {
    decoded = decodeURIComponent(fragment);
  } catch {}
  const at = map.own.indexOf(decoded);
  return at !== -1 && at < map.ids.length ? map.ids[at] : fragment;
}

export function remarkLocaleHeadingIds() {
  return (tree, file) => {
    const from = file?.path ?? file?.history?.[0];
    if (!from) return;
    const fromPath = resolve(from);
    const page = translatedPage(fromPath);

    const map = page && idMap(fromPath);
    let index = 0;
    const walk = (node) => {
      if (map && node.type === 'heading') {
        // Same order as `headingsOf`: the leading h1 is already stripped.
        if (index < map.ids.length) {
          node.data ??= {};
          node.data.hProperties = { ...node.data.hProperties, id: map.ids[index] };
        }
        index++;
      }
      if ((node.type === 'link' || node.type === 'definition') && node.url) {
        const url = node.url;
        const hashAt = url.indexOf('#');
        if (hashAt !== -1 && !/^[a-z][a-z\d+.-]*:/i.test(url) && !url.startsWith('/')) {
          const target = url.slice(0, hashAt);
          const fragment = url.slice(hashAt + 1);
          const targetPath = target ? resolve(dirname(fromPath), target.replace(/\?.*$/, '')) : fromPath;
          if (fragment && translatedPage(targetPath)) node.url = `${target}#${translate(targetPath, fragment)}`;
        }
      }
      for (const child of node.children ?? []) walk(child);
    };
    walk(tree);
  };
}
