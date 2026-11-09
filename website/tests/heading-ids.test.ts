import { expect, test } from 'bun:test';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import { headingsOf, idMap, remarkLocaleHeadingIds } from '../src/lib/remark-locale-heading-ids.js';

// A Chinese page's headings take the English page's ids, so a section link is
// the same, short and readable, in both languages. That needs the two pages to
// have the same outline: the same headings at the same levels, in order.

const ROOT = join(import.meta.dir, '..');
const SECTIONS = ['docs', 'elements', 'modules'];

function pages(): string[] {
  return SECTIONS.flatMap((s) =>
    readdirSync(join(ROOT, 'zh-CN', s))
      .filter((n) => n.endsWith('.md') && existsSync(join(ROOT, s, n)))
      .map((n) => join(ROOT, 'zh-CN', s, n)),
  );
}

test('every Chinese page has the English outline, so every heading gets the English id', () => {
  const drifted = pages().filter((p) => {
    const map = idMap(p);
    return !map || map.ids.length !== map.own.length;
  });
  expect(drifted).toEqual([]);
});

test('Chinese heading ids are ASCII, never a percent-encoded title', () => {
  const nonAscii = pages().flatMap((p) => (idMap(p)?.ids ?? []).filter((id) => !/^[\x20-\x7e]*$/.test(id)).map((id) => `${p}#${id}`));
  expect(nonAscii).toEqual([]);
});

test('section links in Chinese pages point at an English id that exists on the target page', () => {
  const broken: string[] = [];
  for (const path of pages()) {
    const tree = unified().use(remarkParse).parse(readFileSync(path, 'utf8'));
    unified().use(remarkLocaleHeadingIds).runSync(tree, { path, history: [path] } as never);
    const visit = (node: any) => {
      if ((node.type === 'link' || node.type === 'definition') && typeof node.url === 'string') {
        const url: string = node.url;
        const at = url.indexOf('#');
        if (at !== -1 && !/^[a-z][a-z\d+.-]*:/i.test(url) && !url.startsWith('/')) {
          const target = url.slice(0, at) ? resolve(dirname(path), url.slice(0, at)) : path;
          const fragment = url.slice(at + 1);
          if (target.endsWith('.md') && existsSync(target)) {
            const map = idMap(target);
            const ids = map ? map.ids : headingsOf(readFileSync(target, 'utf8')).map((h) => h.slug);
            if (!ids.includes(fragment)) broken.push(`${path.slice(ROOT.length + 1)} → ${url}`);
          }
        }
      }
      for (const child of node.children ?? []) visit(child);
    };
    visit(tree);
  }
  expect(broken).toEqual([]);
});
