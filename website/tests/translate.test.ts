import { describe, expect, test } from 'bun:test';
import { chunks, keepAnchors, mask, reject, unmask } from '../src/lib/translate.js';

const page = '# Title\n\nUse `foo` in [List](../elements/list.md).\n\n> [!NOTE]\n> Hi\n\n```rust\n// a comment\nlet a = 1;\n```\n\n<!-- demo: x -->\n\n## Signals\n';

describe('mask', () => {
  test('hides code, links and comments, and restores them exactly', () => {
    const { text, kept } = mask(page);
    expect(text).not.toContain('foo');
    expect(text).not.toContain('list.md');
    expect(text).not.toContain('a comment');
    expect(text).not.toContain('demo:');
    expect(unmask(text, kept)).toBe(page);
  });
});

describe('reject', () => {
  const { text } = mask(page);
  test('accepts an answer with every token and the same headings', () => {
    expect(reject(text, text.replace('Signals', '시그널').replace('Title', '제목'))).toBeUndefined();
  });
  test('rejects a lost token', () => {
    expect(reject(text, text.replace('⟦0⟧', ''))).toBeDefined();
  });
  test('rejects changed headings', () => {
    expect(reject(text, text.replace('## Signals', 'Signals'))).toBeDefined();
  });
});

describe('keepAnchors', () => {
  test('gives a translated heading its English id', () => {
    expect(keepAnchors('## Signals', '## 시그널')).toBe('<a id="signals"></a>\n\n## 시그널');
  });
  test('leaves the page title first', () => {
    expect(keepAnchors('# Intro', '# 소개')).toBe('# 소개');
  });
  test('adds nothing when the id is the same', () => {
    expect(keepAnchors('## Signals', '## Signals')).toBe('## Signals');
  });
});

test('chunks keep all the text', () => {
  const text = Array.from({ length: 50 }, (_, i) => `## H${i}\n${'x'.repeat(1000)}`).join('\n');
  const pieces = chunks(text, 5000);
  expect(pieces.length).toBeGreaterThan(1);
  expect(pieces.join('\n')).toBe(text);
});
