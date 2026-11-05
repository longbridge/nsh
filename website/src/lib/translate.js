// The parts of `scripts/translate.js` that need no network: what a model may
// not change is masked before it sees a page, and its answer is rejected
// unless everything masked came back.
import GithubSlugger from 'github-slugger';

const token = (n) => `⟦${n}⟧`;

/** Replaces code, HTML, link targets and callout markers with `⟦n⟧` tokens. */
export function mask(markdown) {
  const kept = [];
  const keep = (text) => token(kept.push(text) - 1);
  const lines = markdown.split('\n');
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const fence = /^\s*(?:>\s*)?(`{3,}|~{3,})/.exec(lines[i]);
    if (fence) {
      let end = i + 1;
      while (end < lines.length && !lines[end].replace(/^\s*(?:>\s*)?/, '').startsWith(fence[1])) end++;
      out.push(keep(lines.slice(i, end + 1).join('\n')));
      i = end;
    } else if (/^\s*<\/?[a-zA-Z!]/.test(lines[i])) {
      let end = i;
      while (end + 1 < lines.length && lines[end + 1].trim() !== '') end++;
      out.push(keep(lines.slice(i, end + 1).join('\n')));
      i = end;
    } else {
      out.push(lines[i]);
    }
  }
  const text = out
    .join('\n')
    .replace(/`+[^`\n]+`+/g, keep)
    .replace(/\]\(([^)\n]+)\)/g, (_, target) => `](${keep(target)})`)
    .replace(/\[![A-Z]+\]/g, keep);
  return { text, kept };
}

/** Puts the masked text back. */
export function unmask(text, kept) {
  return text.replace(/⟦(\d+)⟧/g, (_, n) => kept[Number(n)]);
}

const headings = (text) => text.split('\n').filter((line) => /^#{1,6}\s/.test(line));

/** Why a translation of `source` (both masked) must be rejected, or undefined. */
export function reject(source, answer) {
  const tokens = (text) => [...text.matchAll(/⟦\d+⟧/g)].map((m) => m[0]).sort().join();
  if (tokens(source) !== tokens(answer)) return 'masked parts changed';
  const levels = (text) => headings(text).map((line) => line.split(' ')[0]).join();
  if (levels(source) !== levels(answer)) return 'headings changed';
  return undefined;
}

/** Splits masked text at headings into pieces of about `size` characters. */
export function chunks(text, size = 12000) {
  const pieces = [];
  let current = '';
  for (const part of text.split(/\n(?=#{1,3}\s)/)) {
    if (current && current.length + part.length > size) {
      pieces.push(current);
      current = part;
    } else {
      current = current ? `${current}\n${part}` : part;
    }
  }
  if (current) pieces.push(current);
  return pieces;
}

const plainHeading = (line) => unmaskCode(line.replace(/^#{1,6}\s+/, ''));
const unmaskCode = (line) => line.replace(/`/g, '').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');

/**
 * Links name headings by their English ids, so each translated heading gets
 * an anchor with the English one's id too.
 */
export function keepAnchors(english, translated) {
  const en = new GithubSlugger();
  const local = new GithubSlugger();
  const ids = headings(english).map((line) => en.slug(plainHeading(line)));
  let at = 0;
  return translated
    .split('\n')
    .flatMap((line) => {
      if (!/^#{1,6}\s/.test(line)) return [line];
      const id = ids[at++];
      // A page's title is not a link target, and remark-strip-title needs it first.
      return id === undefined || line.startsWith('# ') || id === local.slug(plainHeading(line)) ? [line] : [`<a id="${id}"></a>`, '', line];
    })
    .join('\n');
}

/** A page's frontmatter fields and body; the frontmatter is flat `key: value`. */
export function splitPage(content) {
  const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(content);
  if (!match) return { front: {}, body: content };
  const front = {};
  for (const line of match[1].split(/\r?\n/)) {
    const at = line.indexOf(':');
    if (at < 0) continue;
    const raw = line.slice(at + 1).trim();
    front[line.slice(0, at).trim()] = raw.replace(/^(["'])(.*)\1$/, '$2');
  }
  return { front, body: content.slice(match[0].length) };
}
